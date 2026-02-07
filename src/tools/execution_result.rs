use std::time::Duration;

/// Result of a tool execution including output, errors, and timing
#[derive(Debug, Clone)]
pub struct ToolExecutionResult {
    /// Status: "running", "done", "error"
    pub status: String,
    /// Standard output from tool
    pub output: Option<String>,
    /// Error message if tool failed
    pub error: Option<String>,
    /// Time tool took to execute in milliseconds
    pub execution_time_ms: Option<u64>,
    /// Current attempt number (1-indexed)
    pub attempt: usize,
    /// Maximum attempts allowed
    pub max_attempts: usize,
}

impl ToolExecutionResult {
    /// Create a successful result
    pub fn success(
        output: String,
        execution_time_ms: u64,
        attempt: usize,
        max_attempts: usize,
    ) -> Self {
        Self {
            status: "done".to_string(),
            output: Some(output),
            error: None,
            execution_time_ms: Some(execution_time_ms),
            attempt,
            max_attempts,
        }
    }

    /// Create an error result
    pub fn error(
        error: String,
        execution_time_ms: u64,
        attempt: usize,
        max_attempts: usize,
    ) -> Self {
        Self {
            status: "error".to_string(),
            output: None,
            error: Some(error),
            execution_time_ms: Some(execution_time_ms),
            attempt,
            max_attempts,
        }
    }

    /// Create a running result (no output yet)
    pub fn running(attempt: usize, max_attempts: usize) -> Self {
        Self {
            status: "running".to_string(),
            output: None,
            error: None,
            execution_time_ms: None,
            attempt,
            max_attempts,
        }
    }

    /// Check if this is a successful result
    pub fn is_success(&self) -> bool {
        self.status == "done" && self.error.is_none()
    }

    /// Check if this is an error result
    pub fn is_error(&self) -> bool {
        self.status == "error" || self.error.is_some()
    }

    /// Check if more retries are available
    pub fn can_retry(&self) -> bool {
        self.is_error() && self.attempt < self.max_attempts
    }

    /// Get the next attempt number
    pub fn next_attempt(&self) -> usize {
        self.attempt + 1
    }
}

/// Policy for retrying failed tool executions
#[derive(Debug, Clone)]
pub struct ToolRetryPolicy {
    /// Maximum number of retry attempts
    pub max_retries: usize,
    /// Initial backoff in milliseconds (doubled each retry)
    pub initial_backoff_ms: u64,
    /// Maximum backoff in milliseconds
    pub max_backoff_ms: u64,
}

impl ToolRetryPolicy {
    /// Create policy with custom max retries
    pub fn with_max_retries(max_retries: usize) -> Self {
        Self {
            max_retries,
            initial_backoff_ms: 100,
            max_backoff_ms: 5000,
        }
    }

    /// Calculate backoff duration for given attempt
    pub fn get_backoff(&self, attempt: usize) -> Duration {
        if attempt == 0 {
            return Duration::from_millis(0);
        }

        // Exponential backoff: backoff_ms * 2^(attempt-1)
        let backoff = self.initial_backoff_ms * (1 << (attempt - 1));
        let backoff = backoff.min(self.max_backoff_ms);

        Duration::from_millis(backoff)
    }

    /// Check if retry should be attempted
    pub fn should_retry(&self, attempt: usize, is_error: bool) -> bool {
        is_error && attempt < self.max_retries
    }
}

impl Default for ToolRetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 10,
            initial_backoff_ms: 100,
            max_backoff_ms: 5000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_result_success() {
        let result = ToolExecutionResult::success("output".to_string(), 1000, 1, 10);
        assert!(result.is_success());
        assert!(!result.is_error());
        assert_eq!(result.status, "done");
        assert_eq!(result.output, Some("output".to_string()));
        assert_eq!(result.execution_time_ms, Some(1000));
        assert!(!result.can_retry());
    }

    #[test]
    fn test_execution_result_error() {
        let result = ToolExecutionResult::error("failed".to_string(), 500, 1, 10);
        assert!(!result.is_success());
        assert!(result.is_error());
        assert_eq!(result.status, "error");
        assert_eq!(result.error, Some("failed".to_string()));
        assert!(result.can_retry());
    }

    #[test]
    fn test_execution_result_max_attempts() {
        let result = ToolExecutionResult::error("failed".to_string(), 500, 10, 10);
        assert!(result.is_error());
        assert!(!result.can_retry()); // No more attempts
    }

    #[test]
    fn test_retry_policy_backoff() {
        let policy = ToolRetryPolicy::default();

        // Attempt 1: 100ms
        assert_eq!(policy.get_backoff(1).as_millis(), 100);

        // Attempt 2: 200ms
        assert_eq!(policy.get_backoff(2).as_millis(), 200);

        // Attempt 3: 400ms
        assert_eq!(policy.get_backoff(3).as_millis(), 400);

        // Attempt 4: 800ms
        assert_eq!(policy.get_backoff(4).as_millis(), 800);

        // Attempt 5: 1600ms
        assert_eq!(policy.get_backoff(5).as_millis(), 1600);

        // Attempt 6: 3200ms
        assert_eq!(policy.get_backoff(6).as_millis(), 3200);

        // Attempt 7: 6400ms, but capped at 5000ms (max_backoff)
        assert_eq!(policy.get_backoff(7).as_millis(), 5000);

        // Further attempts still capped at 5000ms
        assert_eq!(policy.get_backoff(8).as_millis(), 5000);
    }

    #[test]
    fn test_retry_policy_should_retry() {
        let policy = ToolRetryPolicy::default();

        // Error on attempt 1: should retry
        assert!(policy.should_retry(1, true));

        // Error on attempt 9: should retry
        assert!(policy.should_retry(9, true));

        // Error on attempt 10: should NOT retry (max reached)
        assert!(!policy.should_retry(10, true));

        // Success: should NOT retry
        assert!(!policy.should_retry(1, false));
    }

    #[test]
    fn test_retry_policy_custom() {
        let policy = ToolRetryPolicy::with_max_retries(3);
        assert_eq!(policy.max_retries, 3);

        assert!(policy.should_retry(1, true)); // Can retry on attempt 1
        assert!(policy.should_retry(2, true)); // Can retry on attempt 2
        assert!(!policy.should_retry(3, true)); // Cannot retry on attempt 3 (at max)
        assert!(!policy.should_retry(4, true)); // Cannot retry on attempt 4 (exceeds max)
    }

    #[test]
    fn test_next_attempt() {
        let result = ToolExecutionResult::error("failed".to_string(), 500, 3, 10);
        assert_eq!(result.attempt, 3);
        assert_eq!(result.next_attempt(), 4);
    }

    #[test]
    fn test_execution_result_running() {
        let result = ToolExecutionResult::running(1, 10);
        assert_eq!(result.status, "running");
        assert!(!result.is_success());
        assert!(!result.is_error());
        assert!(result.output.is_none());
        assert!(result.execution_time_ms.is_none());
    }
}
