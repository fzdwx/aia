#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReasoningEffort {
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
}

impl ReasoningEffort {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Xhigh => "xhigh",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "minimal" => Some(Self::Minimal),
            "low" => Some(Self::Low),
            "medium" => Some(Self::Medium),
            "high" => Some(Self::High),
            "xhigh" => Some(Self::Xhigh),
            _ => None,
        }
    }

    pub(crate) fn parse_optional(value: Option<&str>) -> Result<Option<Self>, String> {
        value
            .map(|value| {
                Self::parse(value).ok_or_else(|| format!("invalid reasoning_effort: {value}"))
            })
            .transpose()
    }

    pub(crate) fn normalize(value: Option<String>) -> Option<String> {
        value.and_then(|value| Self::parse(&value).map(|effort| effort.as_str().to_string()))
    }

    pub(crate) fn normalize_for_model(
        value: Option<String>,
        supports_reasoning: bool,
    ) -> Option<String> {
        if !supports_reasoning {
            return None;
        }
        Self::normalize(value)
    }

    pub(crate) fn serialize_optional(value: Option<Self>) -> Option<String> {
        value.map(|effort| effort.as_str().to_string())
    }
}

#[cfg(test)]
#[path = "../tests/reasoning/mod.rs"]
mod tests;
