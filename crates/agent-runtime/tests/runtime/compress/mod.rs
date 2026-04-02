use super::is_context_length_error;

#[test]
fn detects_openai_context_length_exceeded() {
    assert!(is_context_length_error("Error: context_length_exceeded - max tokens 128000"));
}

#[test]
fn detects_maximum_context_length() {
    assert!(is_context_length_error("This model's maximum context length is 128000 tokens"));
}

#[test]
fn detects_too_many_tokens() {
    assert!(is_context_length_error("Request has too many tokens"));
}

#[test]
fn detects_context_window() {
    assert!(is_context_length_error("Input exceeds the context window limit"));
}

#[test]
fn detects_input_longer_than_model_context_length() {
    assert!(is_context_length_error(
        "The input (227574 tokens) is longer than the model's context length (202752 tokens)"
    ));
}

#[test]
fn does_not_match_unrelated_errors() {
    assert!(!is_context_length_error("rate limit exceeded"));
    assert!(!is_context_length_error("internal server error"));
}
