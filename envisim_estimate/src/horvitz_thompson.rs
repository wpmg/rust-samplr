// Copyright (C) 2024 Wilmer Prentius, Anton Grafström.
//
// This program is free software: you can redistribute it and/or modify it under the terms of the
// GNU Affero General Public License as published by the Free Software Foundation, version 3.
//
// This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without
// even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License along with this
// program. If not, see <https://www.gnu.org/licenses/>.

//! Horvitz-Thompson estimators (single count estimators)

use envisim_samplr::SamplingError;
use envisim_utils::kd_tree::{Searcher, TreeBuilder};
use envisim_utils::utils::{sum, usize_to_f64};
use envisim_utils::{InputError, Matrix, Probabilities};
use std::num::NonZeroUsize;

/// Horvitz-Thompson estimator of a total
///
/// # Examples
/// ```
/// use envisim_estimate::horvitz_thompson::estimate;
///
/// let y = [0.0, 0.1, 0.2, 0.3, 0.4];
/// let pi = [0.2; 5];
///
/// estimate(&y, &pi).unwrap(); // Should be about 5.0
/// ```
#[inline]
pub fn estimate(y_values: &[f64], probabilities: &[f64]) -> Result<f64, SamplingError> {
    InputError::check_lengths(y_values, probabilities).and(Probabilities::check(probabilities))?;

    Ok(y_values
        .iter()
        .zip(probabilities.iter())
        .fold(0.0, |acc, (&y, &p)| acc + y / p))
}

/// Ratio estimator of total, using auxilliary variable `x_values`.
#[inline]
pub fn ratio(
    y_values: &[f64],
    x_values: &[f64],
    probabilities: &[f64],
    x_total: f64,
) -> Result<f64, SamplingError> {
    InputError::check_range_f64(x_total, 0.0, f64::INFINITY)?;
    Ok(estimate(y_values, probabilities)? / estimate(x_values, probabilities)? * x_total)
}

/// Horvitz-Thompson estimator of variance of total estimate
pub fn variance(
    y_values: &[f64],
    probabilities: &[f64],
    probabilities_second_order: &Matrix,
) -> Result<f64, SamplingError> {
    let sample_size = y_values.len();
    InputError::check_lengths(y_values, probabilities)
        .and(InputError::check_sizes(
            sample_size,
            probabilities_second_order.nrow(),
        ))
        .and(InputError::check_sizes(
            sample_size,
            probabilities_second_order.ncol(),
        ))
        .and(Probabilities::check(probabilities))
        .and(Probabilities::check(probabilities_second_order.data()))?;

    let mut variance: f64 = 0.0;

    for i in 0..sample_size {
        let y_pi = y_values[i] / probabilities[i];
        variance += y_pi.powi(2) * (1.0 - probabilities[i]);

        for j in (i + 1)..sample_size {
            variance += 2.0 * y_pi * y_values[j] / probabilities[j]
                * (1.0 - probabilities[i] * probabilities[j] / probabilities_second_order[(i, j)]);
        }
    }

    Ok(variance)
}

/// Sen-Yates-Grundy estimator of variance of total estimate of fixed sized sample
pub fn syg_variance(
    y_values: &[f64],
    probabilities: &[f64],
    probabilities_second_order: &Matrix,
) -> Result<f64, SamplingError> {
    let sample_size = y_values.len();
    InputError::check_lengths(y_values, probabilities)
        .and(InputError::check_sizes(
            sample_size,
            probabilities_second_order.nrow(),
        ))
        .and(InputError::check_sizes(
            sample_size,
            probabilities_second_order.ncol(),
        ))
        .and(Probabilities::check(probabilities))
        .and(Probabilities::check(probabilities_second_order.data()))?;

    let mut variance: f64 = 0.0;

    for i in 0..sample_size {
        let y_pi = y_values[i] / probabilities[i];

        for j in (i + 1)..sample_size {
            variance -= (y_pi - y_values[j] / probabilities[j]).powi(2)
                * (1.0 - probabilities[i] * probabilities[j] / probabilities_second_order[(i, j)]);
        }
    }

    Ok(variance)
}

/// Deville estimator of variance of total estimate
pub fn deville_variance(y_values: &[f64], probabilities: &[f64]) -> Result<f64, SamplingError> {
    InputError::check_lengths(y_values, probabilities).and(Probabilities::check(probabilities))?;

    let y_pi: Vec<f64> = y_values
        .iter()
        .zip(probabilities.iter())
        .map(|(&y, &p)| y / p)
        .collect();

    let q: Vec<f64> = probabilities.iter().map(|&p| 1.0 - p).collect();

    let s1mp = sum(&q);
    let del = y_pi
        .iter()
        .zip(q.iter())
        .fold(0.0, |acc, (&a, &b)| acc + a * b);
    let s1mp_del = s1mp / del;
    let sak2 = q.iter().fold(0.0, |acc, &a| acc + a.powi(2)) / s1mp.powi(2);

    let dsum = y_pi
        .iter()
        .zip(q.iter())
        .fold(0.0, |acc, (&a, &b)| acc + (a - s1mp_del).powi(2) * b);

    Ok(1.0 / (1.0 - sak2) * dsum)
}

/// Local mean estimator of variance of total estimate.
///
/// # References
/// Grafström, A., & Schelin, L. (2014).
/// How to select representative samples.
/// Scandinavian Journal of Statistics, 41(2), 277-290.
/// <https://doi.org/10.1111/sjos.12016>
pub fn local_mean_variance(
    y_values: &[f64],
    probabilities: &[f64],
    tree_builder: &TreeBuilder,
    n_neighbours: NonZeroUsize,
) -> Result<f64, SamplingError> {
    let sample_size = y_values.len();
    let tree = tree_builder.build(&mut (0..sample_size).collect::<Vec<usize>>())?;
    let mut searcher = Searcher::new(&tree, n_neighbours);
    let auxilliaries = tree.data();

    InputError::check_lengths(y_values, probabilities)
        .and(InputError::check_sizes(sample_size, auxilliaries.nrow()))
        .and(Probabilities::check(probabilities))?;

    let yp: Vec<f64> = y_values
        .iter()
        .zip(probabilities.iter())
        .map(|(&y, &p)| y / p)
        .collect();
    let mut variance: f64 = 0.0;

    for i in 0..sample_size {
        searcher
            .find_neighbours_of_iter(&tree, &mut auxilliaries.row_iter(i))
            .unwrap();
        let len = usize_to_f64(searcher.neighbours().len());
        variance += len / (len - 1.0)
            * (searcher
                .neighbours()
                .iter()
                .fold(0.0, |acc, &id| acc + yp[id])
                / len)
                .powi(2);
    }

    Ok(variance)
}
