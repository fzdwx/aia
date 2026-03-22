use std::path::Path;

use agent_prompts::{AiaAgentsPromptContext, SystemPromptBlock, build_system_prompt};
use time::{Month, OffsetDateTime, UtcOffset, Weekday};

pub(super) fn build_session_system_prompt(
    system_prompt: Option<&str>,
    workspace_root: &Path,
) -> String {
    match system_prompt.map(str::trim).filter(|prompt| !prompt.is_empty()) {
        Some(prompt) => prompt.to_string(),
        None => {
            let config = agent_prompts::SystemPromptConfig::default().with_context_block(
                SystemPromptBlock::new(
                    "Context Contract",
                    agent_prompts::context_contract(
                        agent_prompts::AGENT_HANDOFF_THRESHOLD,
                        agent_prompts::AUTO_COMPRESSION_THRESHOLD,
                    ),
                ),
            );
            build_system_prompt(&default_aia_agents_prompt(workspace_root), &config)
        }
    }
}

fn default_aia_agents_prompt(workspace_root: &Path) -> String {
    let context = runtime_prompt_context(workspace_root);
    agent_prompts::render_aia_agents_prompt(context)
}

fn runtime_prompt_context(workspace_root: &Path) -> AiaAgentsPromptContext {
    let offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
    let now = OffsetDateTime::now_utc().to_offset(offset);

    AiaAgentsPromptContext {
        platform: std::env::consts::OS.to_string(),
        working_directory: workspace_root.display().to_string(),
        local_date: format_local_date(now),
        weekday: weekday_name(now.weekday()).to_string(),
        timezone: format_utc_offset(offset),
    }
}

fn format_local_date(datetime: OffsetDateTime) -> String {
    format!("{:04}-{:02}-{:02}", datetime.year(), month_number(datetime.month()), datetime.day())
}

fn month_number(month: Month) -> u8 {
    match month {
        Month::January => 1,
        Month::February => 2,
        Month::March => 3,
        Month::April => 4,
        Month::May => 5,
        Month::June => 6,
        Month::July => 7,
        Month::August => 8,
        Month::September => 9,
        Month::October => 10,
        Month::November => 11,
        Month::December => 12,
    }
}

fn weekday_name(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Monday => "Monday",
        Weekday::Tuesday => "Tuesday",
        Weekday::Wednesday => "Wednesday",
        Weekday::Thursday => "Thursday",
        Weekday::Friday => "Friday",
        Weekday::Saturday => "Saturday",
        Weekday::Sunday => "Sunday",
    }
}

fn format_utc_offset(offset: UtcOffset) -> String {
    let total_seconds = offset.whole_seconds();
    let sign = if total_seconds < 0 { '-' } else { '+' };
    let absolute_seconds = total_seconds.abs();
    let hours = absolute_seconds / 3_600;
    let minutes = (absolute_seconds % 3_600) / 60;
    if minutes == 0 {
        format!("UTC{sign}{hours}")
    } else {
        format!("UTC{sign}{hours}:{minutes:02}")
    }
}

#[cfg(test)]
#[path = "../../tests/session_manager/prompt/mod.rs"]
mod tests;
