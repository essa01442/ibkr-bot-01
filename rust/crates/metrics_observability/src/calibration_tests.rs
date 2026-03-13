#[cfg(test)]
mod tests {
    use crate::{CalibrationField, CalibrationLogger};

    #[test]
    fn test_predicted_slippage_zero_fails() {
        let result = CalibrationField::new_real(0.0);
        assert!(result.is_err(), "Creating Real CalibrationField with 0.0 should return an error");
        assert_eq!(result.unwrap_err(), "predicted_slippage cannot be 0.0 for non-zero expected price");
    }

    #[test]
    fn test_unavailable_empty_reason_fails() {
        let result = CalibrationField::new_unavailable("   ");
        assert!(result.is_err(), "Creating Unavailable CalibrationField with empty reason should return an error");
        assert_eq!(result.unwrap_err(), "Unavailable reason cannot be empty");
    }

    #[test]
    fn test_real_slippage_computation() {
        let mut logger = CalibrationLogger::new(1);

        let expected_price: f64 = 10.0;
        let fill_price: f64 = 10.05; // 0.05 actual slip

        // Let's assume the predicted slip model expected 0.02.
        let predicted_slip: f64 = 0.02;

        let predicted_field = CalibrationField::new_real(predicted_slip).unwrap();
        let actual_slip: f64 = (fill_price - expected_price).abs(); // 0.05

        logger.record(1, 123456789, 100, expected_price, predicted_field, actual_slip);

        // Ratio should be actual / predicted = 0.05 / 0.02 = 2.5
        let eval_ratio = logger.evaluate();
        assert!(eval_ratio.is_some());
        assert!((eval_ratio.unwrap() - 2.5).abs() < 1e-9);
    }
}
