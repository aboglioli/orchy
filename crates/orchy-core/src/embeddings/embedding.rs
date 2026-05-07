use serde::{Deserialize, Serialize};

use crate::error::{DomainError, DomainResult};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Embedding {
    values: Vec<f32>,
    model: String,
    dimensions: u32,
}

impl Embedding {
    pub fn new(values: Vec<f32>, model: String, expected_dimensions: u32) -> DomainResult<Self> {
        if model.trim().is_empty() {
            return Err(DomainError::validation("embedding model must not be empty"));
        }
        if expected_dimensions == 0 {
            return Err(DomainError::validation(
                "embedding dimensions must be positive",
            ));
        }
        if values.len() as u32 != expected_dimensions {
            return Err(DomainError::validation(format!(
                "embedding dimension mismatch: got {}, expected {}",
                values.len(),
                expected_dimensions
            )));
        }
        for (i, v) in values.iter().enumerate() {
            if !v.is_finite() {
                return Err(DomainError::validation(format!(
                    "embedding value at index {i} is not finite: {v}"
                )));
            }
        }
        Ok(Self {
            values,
            model,
            dimensions: expected_dimensions,
        })
    }

    pub fn values(&self) -> &[f32] {
        &self.values
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn dimensions(&self) -> u32 {
        self.dimensions
    }

    pub fn into_values(self) -> Vec<f32> {
        self.values
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_rejects_nan() {
        let result = Embedding::new(vec![1.0, f32::NAN], "m".into(), 2);
        assert!(result.is_err());
    }

    #[test]
    fn new_rejects_positive_infinity() {
        let result = Embedding::new(vec![1.0, f32::INFINITY], "m".into(), 2);
        assert!(result.is_err());
    }

    #[test]
    fn new_rejects_negative_infinity() {
        let result = Embedding::new(vec![1.0, f32::NEG_INFINITY], "m".into(), 2);
        assert!(result.is_err());
    }

    #[test]
    fn new_rejects_dimension_mismatch() {
        let result = Embedding::new(vec![1.0, 2.0, 3.0], "m".into(), 4);
        assert!(result.is_err());
    }

    #[test]
    fn new_rejects_empty_model() {
        let result = Embedding::new(vec![1.0], " ".into(), 1);
        assert!(result.is_err());
    }

    #[test]
    fn new_accepts_finite_values() {
        let emb = Embedding::new(vec![0.5, -0.5], "m".into(), 2).unwrap();
        assert_eq!(emb.values(), &[0.5, -0.5]);
        assert_eq!(emb.model(), "m");
        assert_eq!(emb.dimensions(), 2);
    }
}
