use std::fmt;

/// A small interface for a system of ordinary differential equations.
pub trait DynamicalSystem {
    fn derivative(&self, time: f64, state: &[f64], output: &mut [f64]);
}

/// Errors returned by the built-in numerical integration helpers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SimulationError {
    StateDerivativeSizeMismatch,
    NonPositiveStep,
}

impl fmt::Display for SimulationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StateDerivativeSizeMismatch => {
                write!(formatter, "state and derivative sizes differ")
            }
            Self::NonPositiveStep => write!(formatter, "simulation step must be positive"),
        }
    }
}

impl std::error::Error for SimulationError {}

/// Performs one explicit Euler step.
pub fn euler_step<S: DynamicalSystem>(
    system: &S,
    time: f64,
    step: f64,
    state: &mut [f64],
) -> Result<(), SimulationError> {
    if step <= 0.0 {
        return Err(SimulationError::NonPositiveStep);
    }

    let mut derivative = vec![0.0; state.len()];
    system.derivative(time, state, &mut derivative);
    if derivative.len() != state.len() {
        return Err(SimulationError::StateDerivativeSizeMismatch);
    }

    for (value, slope) in state.iter_mut().zip(derivative) {
        *value += slope * step;
    }
    Ok(())
}

/// Tracks simulation time independently from render time.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SimulationClock {
    pub time: f64,
    pub step_index: u64,
}

impl SimulationClock {
    pub fn advance(&mut self, step: f64) -> Result<(), SimulationError> {
        if step <= 0.0 {
            return Err(SimulationError::NonPositiveStep);
        }
        self.time += step;
        self.step_index = self.step_index.saturating_add(1);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{DynamicalSystem, SimulationError, euler_step};

    struct ConstantAcceleration;

    impl DynamicalSystem for ConstantAcceleration {
        fn derivative(&self, _time: f64, _state: &[f64], output: &mut [f64]) {
            output.fill(2.0);
        }
    }

    #[test]
    fn euler_step_updates_state() {
        let mut state = [1.0, -1.0];
        euler_step(&ConstantAcceleration, 0.0, 0.5, &mut state).expect("valid step");
        assert_eq!(state, [2.0, 0.0]);
    }

    #[test]
    fn euler_step_rejects_invalid_delta() {
        let mut state = [0.0];
        assert_eq!(
            euler_step(&ConstantAcceleration, 0.0, 0.0, &mut state),
            Err(SimulationError::NonPositiveStep)
        );
    }
}
