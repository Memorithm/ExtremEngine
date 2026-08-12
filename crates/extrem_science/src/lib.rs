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

/// Performs one classical fourth-order Runge-Kutta step.
pub fn rk4_step<S: DynamicalSystem>(
    system: &S,
    time: f64,
    step: f64,
    state: &mut [f64],
) -> Result<(), SimulationError> {
    if step <= 0.0 {
        return Err(SimulationError::NonPositiveStep);
    }
    let mut k1 = vec![0.0; state.len()];
    let mut k2 = vec![0.0; state.len()];
    let mut k3 = vec![0.0; state.len()];
    let mut k4 = vec![0.0; state.len()];
    let mut scratch = state.to_vec();
    system.derivative(time, state, &mut k1);
    for ((value, original), slope) in scratch.iter_mut().zip(state.iter()).zip(k1.iter()) {
        *value = *original + slope * step * 0.5;
    }
    system.derivative(time + step * 0.5, &scratch, &mut k2);
    for ((value, original), slope) in scratch.iter_mut().zip(state.iter()).zip(k2.iter()) {
        *value = *original + slope * step * 0.5;
    }
    system.derivative(time + step * 0.5, &scratch, &mut k3);
    for ((value, original), slope) in scratch.iter_mut().zip(state.iter()).zip(k3.iter()) {
        *value = *original + slope * step;
    }
    system.derivative(time + step, &scratch, &mut k4);
    for index in 0..state.len() {
        state[index] += step * (k1[index] + 2.0 * k2[index] + 2.0 * k3[index] + k4[index]) / 6.0;
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
    use super::{DynamicalSystem, SimulationError, euler_step, rk4_step};

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

    #[test]
    fn rk4_step_integrates_constant_acceleration() {
        let mut state = [1.0];
        rk4_step(&ConstantAcceleration, 0.0, 0.5, &mut state).expect("valid step");
        assert!((state[0] - 2.0).abs() < 0.000_001);
    }
}
