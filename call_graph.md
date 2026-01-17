# Osmogrep Agent Map

> Auto-generated. Do not edit manually.

## Files

### src/agent.rs

**Functions**
- fn save_config(cfg: &Config) -> std::io::Result<()>
-  pub fn new() -> Self
-  pub fn set_api_key(&mut self, key: String)
-  pub fn spawn( &self, repo_root: PathBuf, user_text: String, tx: Sender<AgentEvent>, )
-  fn run( &self, _repo_root: PathBuf, user_text: &str, tx: &Sender<AgentEvent>, ) -> Result<(), String>
-  fn call_openai( &self, api_key: &str, input: &Value, ) -> Result<Value, String>

**Calls**
- AgentEvent::Error
- AgentEvent::OutputText
- Command::new
- PathBuf::from
- Stdio::piped
- String::new
- ToolRegistry::new
- Value::Array
- Value::String
- dirs::config_dir
- env::var
- fs::create_dir_all
- fs::read_to_string
- fs::write
- serde_json::from_str
- serde_json::to_string
- thread::spawn
- toml::from_str
- toml::to_string

### src/commands.rs

**Functions**
- pub fn handle_command(state: &mut AgentState, raw: &str)
- pub fn update_command_hints(state: &mut AgentState)

**Calls**
- Instant::now

### src/logger.rs

**Functions**
- pub fn log( state: &mut AgentState, level: LogLevel, msg: impl Into<String>, )
- pub fn log_user_input( state: &mut AgentState, input: impl Into<String>, )
- pub fn log_tool_call( state: &mut AgentState, tool: impl Into<String>, args: impl Into<String>, )
- pub fn log_tool_result( state: &mut AgentState, msg: impl Into<String>, )
- pub fn log_agent_output( state: &mut AgentState, text: &str, )

**Calls**

### src/main.rs

**Functions**
- fn run_shell(state: &mut AgentState, cmd: &str)
- fn main() -> Result<(), Box<dyn Error>>
- fn init_state() -> AgentState
- fn teardown_terminal( terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, ) -> Result<(), Box<dyn Error>>

**Calls**
- Agent::new
- AgentEvent::Error
- AgentEvent::OutputText
- Command::new
- CrosstermBackend::new
- Duration::from_millis
- Instant::now
- LogBuffer::new
- Rect::default
- String::from_utf8_lossy
- String::new
- Terminal::new
- Vec::new
- commands::handle_command
- commands::update_command_hints
- env::current_dir
- event::poll
- event::read
- io::stdout
- mpsc::channel

### src/state.rs

**Functions**
-  pub fn new() -> Self
-  pub fn push(&mut self, level: LogLevel, text: impl Into<String>)
-  pub fn iter(&self) -> impl Iterator<Item = &LogLine>
-  pub fn history_prev(&mut self)
-  pub fn history_next(&mut self)
-  pub fn commit_input(&mut self) -> String

**Calls**
- Instant::now
- VecDeque::with_capacity
- logger::log

### src/tools/edit.rs

**Functions**
-  fn schema(&self) -> Value
-  fn call(&self, args: Value) -> ToolResult

**Calls**
- fs::read_to_string
- fs::write

### src/tools/glob.rs

**Functions**
-  fn schema(&self) -> Value
-  fn call(&self, args: Value) -> ToolResult

**Calls**
- Vec::new
- WalkDir::new

### src/tools/mod.rs

**Functions**
-  fn call(&self, args: Value) -> ToolResult; } pub struct ToolRegistry
-  pub fn new() -> Self
-  pub fn call(&self, name: &str, args: Value) -> ToolResult

**Calls**
- Box::new
- HashMap::new

### src/tools/read.rs

**Functions**
-  fn schema(&self) -> Value
-  fn call(&self, args: Value) -> ToolResult

**Calls**
- fs::read_to_string

### src/tools/search.rs

**Functions**
-  fn schema(&self) -> Value
-  fn call(&self, args: Value) -> ToolResult

**Calls**
- BufReader::new
- Command::new
- File::open
- String::from_utf8_lossy
- Vec::new
- WalkDir::new

### src/tools/shell.rs

**Functions**
-  fn schema(&self) -> Value
-  fn call(&self, args: Value) -> ToolResult

**Calls**
- Command::new
- String::from_utf8_lossy

### src/tools/write.rs

**Functions**
-  fn schema(&self) -> Value
-  fn call(&self, args: Value) -> ToolResult

**Calls**
- fs::write

### src/ui/main_ui.rs

**Functions**
- fn parse_input(raw: &str) -> InputMode
- pub fn handle_event( state: &mut AgentState, event: impl Into<Event>, _input_rect: Rect, _diff_rect: Rect, _exec_rect: Rect, )
- fn handle_key(state: &mut AgentState, k: KeyEvent)
- fn handle_mouse(state: &mut AgentState, m: MouseEvent)

**Calls**
- Event::Key
- Event::Mouse
- KeyCode::Char

### src/ui/mod.rs

**Functions**

**Calls**

### src/ui/tui.rs

**Functions**
- pub fn draw_ui<B: Backend>( terminal: &mut Terminal<B>, state: &AgentState, ) -> io::Result<(Rect, Rect, Rect)>
- fn render_header(f: &mut ratatui::Frame, area: Rect)
- fn render_execution( f: &mut ratatui::Frame, area: Rect, state: &AgentState, )
- fn render_input( f: &mut ratatui::Frame, area: Rect, state: &AgentState, )
- fn render_status( f: &mut ratatui::Frame, area: Rect, state: &AgentState, )
- fn render_command_box_line(text: &str, with_prefix: bool) -> Line
- fn running_pulse(start: Option<Instant>) -> Option<String>

**Calls**
- Block::default
- Color::Rgb
- Constraint::Length
- Constraint::Min
- Layout::default
- Line::from
- Paragraph::new
- Rect::default
- Span::raw
- Span::styled
- String::with_capacity
- Style::default
- Vec::new

## Global Hotspots

### Thread creation
- src/agent.rs → thread::spawn

### Process execution
- src/agent.rs → Command::new
- src/main.rs → Command::new
- src/tools/search.rs → Command::new
- src/tools/shell.rs → Command::new

### AgentEvent fan-out
- src/agent.rs
- src/main.rs

