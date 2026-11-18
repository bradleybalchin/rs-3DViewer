//! # Farthest Point Sampling
//! 
//! Randomly choose point p, and add to selected set A,
//! Each iteration, choose new point p not in A that is furthest away from any point in A
//! Repeat until A has target point number

use crate::render::vertex::GpuPoint;

/// Sample `points` using Farthest Point Sampling to a target size `target`
/// 
/// This function is `O(N * M)` for `N` points and target sample size `M` 
/// 
/// ### Method
/// 1. Randomly select some point p ∈ Points
/// 2. Add to sample set A
/// 3. For each point in P \ A, calculate the min distance to any point in A
/// 4. Add the point with the highest min distance to A
/// 5. repeat steps 3 and 4 until A has the target number of points 
fn sample(points: &Vec<GpuPoint>, target: usize) {

}



#[cfg(test)]
mod tests { 

}