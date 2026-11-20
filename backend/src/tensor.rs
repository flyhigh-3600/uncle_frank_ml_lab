//! Tiny row-major tensor. No autograd crate, no safety rails.

use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Tensor {
    pub shape: Vec<usize>,
    pub data: Vec<f32>,
}

pub type TResult = Result<Tensor, String>;

impl Tensor {
    pub fn new(shape: Vec<usize>, data: Vec<f32>) -> TResult {
        let want: usize = shape.iter().product();
        if want != data.len() {
            return Err(format!(
                "shape {:?} needs {} values, got {}",
                shape,
                want,
                data.len()
            ));
        }
        Ok(Self { shape, data })
    }

    pub fn zeros(shape: Vec<usize>) -> Self {
        let n: usize = shape.iter().product();
        Self {
            shape,
            data: vec![0.0; n],
        }
    }

    pub fn scalar(v: f32) -> Self {
        Self {
            shape: vec![1],
            data: vec![v],
        }
    }

    pub fn rows(&self) -> usize {
        self.shape.first().copied().unwrap_or(1)
    }

    pub fn cols(&self) -> usize {
        self.shape.get(1).copied().unwrap_or(1)
    }

    pub fn at(&self, i: usize, j: usize) -> f32 {
        self.data[i * self.cols() + j]
    }

    pub fn set(&mut self, i: usize, j: usize, v: f32) {
        let c = self.cols();
        self.data[i * c + j] = v;
    }

    pub fn matmul(&self, rhs: &Tensor) -> TResult {
        if self.shape.len() != 2 || rhs.shape.len() != 2 {
            return Err("matmul wants 2-D tensors".into());
        }
        let (m, k) = (self.rows(), self.cols());
        if rhs.rows() != k {
            return Err(format!(
                "matmul mismatch: {:?} x {:?}",
                self.shape, rhs.shape
            ));
        }
        let n = rhs.cols();
        let mut out = vec![0.0f32; m * n];
        for i in 0..m {
            for p in 0..k {
                let a = self.data[i * k + p];
                if a == 0.0 {
                    continue;
                }
                for j in 0..n {
                    out[i * n + j] += a * rhs.data[p * n + j];
                }
            }
        }
        Tensor::new(vec![m, n], out)
    }

    /// self + bias broadcast over columns.
    pub fn add_bias(&self, bias: &Tensor) -> TResult {
        if bias.data.len() != self.cols() {
            return Err(format!(
                "bias len {} != cols {}",
                bias.data.len(),
                self.cols()
            ));
        }
        let (n, c) = (self.rows(), self.cols());
        let mut data = self.data.clone();
        for i in 0..n {
            for j in 0..c {
                data[i * c + j] += bias.data[j];
            }
        }
        Ok(Tensor {
            shape: vec![n, c],
            data,
        })
    }

    pub fn relu(&self) -> Tensor {
        Tensor {
            shape: self.shape.clone(),
            data: self.data.iter().map(|v| v.max(0.0)).collect(),
        }
    }

    pub fn tanh(&self) -> Tensor {
        Tensor {
            shape: self.shape.clone(),
            data: self.data.iter().map(|v| v.tanh()).collect(),
        }
    }

    pub fn softmax(&self) -> TResult {
        if self.shape.len() != 2 {
            return Err("softmax wants a 2-D tensor".into());
        }
        let (n, c) = (self.rows(), self.cols());
        let mut data = self.data.clone();
        for i in 0..n {
            let row = &mut data[i * c..(i + 1) * c];
            let max = row.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            let mut sum = 0.0;
            for v in row.iter_mut() {
                *v = (*v - max).exp();
                sum += *v;
            }
            for v in row.iter_mut() {
                *v /= sum.max(1e-12);
            }
        }
        Ok(Tensor {
            shape: vec![n, c],
            data,
        })
    }

    /// Row-wise argmax.
    pub fn argmax(&self) -> Vec<usize> {
        let (n, c) = (self.rows(), self.cols());
        (0..n)
            .map(|i| {
                let row = &self.data[i * c..(i + 1) * c];
                let mut best = 0;
                for j in 1..c {
                    if row[j] > row[best] {
                        best = j;
                    }
                }
                best
            })
            .collect()
    }

    pub fn mean(&self) -> f32 {
        if self.data.is_empty() {
            return 0.0;
        }
        self.data.iter().sum::<f32>() / self.data.len() as f32
    }

    pub fn min(&self) -> f32 {
        self.data.iter().cloned().fold(f32::INFINITY, f32::min)
    }

    pub fn max(&self) -> f32 {
        self.data.iter().cloned().fold(f32::NEG_INFINITY, f32::max)
    }

    pub fn has_nan(&self) -> bool {
        self.data.iter().any(|v| !v.is_finite())
    }
}

/// Mean squared error between two same-shaped tensors.
pub fn mse(pred: &Tensor, target: &Tensor) -> Result<(f32, Tensor, Tensor), String> {
    if pred.shape != target.shape {
        return Err(format!(
            "mse shape mismatch: {:?} vs {:?}",
            pred.shape, target.shape
        ));
    }
    let n = pred.data.len() as f32;
    let mut dl = vec![0.0; pred.data.len()];
    let mut loss = 0.0;
    for i in 0..pred.data.len() {
        let d = pred.data[i] - target.data[i];
        loss += d * d;
        dl[i] = 2.0 * d / n;
    }
    let d_target: Vec<f32> = dl.iter().map(|v| -v).collect();
    Ok((
        loss / n,
        Tensor::new(pred.shape.clone(), dl)?,
        Tensor::new(target.shape.clone(), d_target)?,
    ))
}

/// Softmax cross-entropy over logits. Targets may be one-hot or distributions.
pub fn cross_entropy(logits: &Tensor, target: &Tensor) -> Result<(f32, Tensor, Tensor), String> {
    if logits.shape != target.shape || logits.shape.len() != 2 {
        return Err(format!(
            "cross-entropy shape mismatch: {:?} vs {:?}",
            logits.shape, target.shape
        ));
    }
    let probs = logits.softmax()?;
    let (n, c) = (logits.rows(), logits.cols());
    let mut loss = 0.0;
    for i in 0..n {
        for j in 0..c {
            let t = target.at(i, j);
            if t > 0.0 {
                loss -= t * probs.at(i, j).max(1e-12).ln();
            }
        }
    }
    loss /= n as f32;
    let mut dl = vec![0.0; n * c];
    let mut dt = vec![0.0; n * c];
    for i in 0..n {
        for j in 0..c {
            let g = (probs.at(i, j) - target.at(i, j)) / n as f32;
            dl[i * c + j] = g;
            dt[i * c + j] = -g;
        }
    }
    Ok((
        loss,
        Tensor::new(vec![n, c], dl)?,
        Tensor::new(vec![n, c], dt)?,
    ))
}

/// Deterministic xorshift/ splitmix PRNG so runs are reproducible.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub fn unit(&mut self) -> f32 {
        (self.next_u64() >> 11) as f32 / (1u64 << 53) as f32
    }

    /// Standard normal via Box-Muller.
    pub fn gauss(&mut self) -> f32 {
        let u1 = self.unit().max(1e-7);
        let u2 = self.unit();
        (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos()
    }

    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }
}

/// Stable seed from a node id so weights persist across requests.
pub fn hash_seed(s: &str, salt: u64) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325 ^ salt;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}
