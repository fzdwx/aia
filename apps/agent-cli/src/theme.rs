use ratatui::style::{Color, Modifier, Style};

/// 用户标签行：Cyan + Bold
pub const USER_LABEL_STYLE: Style = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);

/// 助手标签行：Green + Bold
pub const ASSISTANT_LABEL_STYLE: Style = Style::new().fg(Color::Green).add_modifier(Modifier::BOLD);

/// 工具调用：Yellow 前景 + Dim
pub const TOOL_STYLE: Style = Style::new().fg(Color::Yellow).add_modifier(Modifier::DIM);

/// 工具调用失败：Red 前景
pub const TOOL_FAIL_STYLE: Style = Style::new().fg(Color::Red);

/// 轮次失败：Red + Bold
pub const FAIL_STYLE: Style = Style::new().fg(Color::Red).add_modifier(Modifier::BOLD);

pub const MARKDOWN_HEADING_STYLE: Style =
    Style::new().fg(Color::Green).add_modifier(Modifier::BOLD);

pub const MARKDOWN_INLINE_CODE_STYLE: Style = Style::new().fg(Color::Yellow);

pub const MARKDOWN_CODE_BLOCK_STYLE: Style =
    Style::new().fg(Color::Yellow).add_modifier(Modifier::DIM);

pub const MARKDOWN_BULLET_STYLE: Style = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);

pub const MARKDOWN_QUOTE_STYLE: Style = Style::new().fg(Color::DarkGray);

/// 提示/快捷键/次要信息：Dim
pub const DIM_STYLE: Style = Style::new().add_modifier(Modifier::DIM);

/// 分隔线：DarkGray
pub const SEPARATOR_STYLE: Style = Style::new().fg(Color::DarkGray);

/// 处理中指示：Cyan + Bold
pub const SPINNER_STYLE: Style = Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD);

/// Braille spinner 帧序列
pub const SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
