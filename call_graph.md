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

### src/context/indexer.rs

**Functions**
- pub fn load_or_build(root: impl AsRef<Path>) -> Context
- fn should_ignore(path: &Path) -> bool
- fn detect_language(path: &Path) -> Option<&'static str>
- fn compute_file_hashes(root: &Path) -> HashMap<String, String>
- fn compute_repo_stats(root: &Path) -> RepoStats
- fn incremental_update( root: &Path, ctx: &mut Context, old: &HashMap<String, String>, new: &HashMap<String, String>, )
- fn collect_calls(node: Node, src: &str, out: &mut HashSet<String>)
- fn python_doc(node: Node, src: &str) -> Option<String>
- fn rust_doc(node: Node, src: &str) -> Option<String>
- fn build_context( root: &Path, stats: RepoStats, hashes: &HashMap<String, String>, ) -> Context
- fn extract_python(src: &str, file: &str, out: &mut Vec<Symbol>)
- fn extract_python_fn(n: Node, src: &str, file: &str) -> Symbol
- fn extract_python_class(n: Node, src: &str, file: &str) -> Symbol
- fn extract_rust(src: &str, file: &str, out: &mut Vec<Symbol>)
- fn finalize_calls(symbols: &mut Vec<Symbol>)

**Calls**
- HashMap::new
- HashSet::new
- Hasher::new
- Parser::new
- Path::new
- Vec::new
- WalkDir::new
- fs::create_dir_all
- fs::metadata
- fs::read
- fs::read_to_string
- fs::write
- python::language
- rust::language
- serde_json::to_string_pretty

### src/context/mod.rs

**Functions**
- pub fn spawn_indexer( root: PathBuf, tx: Sender<ContextEvent>, )

**Calls**
- ContextEvent::Error
- panic::catch_unwind
- thread::spawn

### src/logger.rs

**Functions**
- pub fn log( state: &mut AgentState, level: LogLevel, msg: impl Into<String>, )
- pub fn log_user_input( state: &mut AgentState, input: impl Into<String>, )
- fn format_tool_name(raw: &str) -> &str
- pub fn log_tool_call( state: &mut AgentState, tool: impl AsRef<str>, command: impl Into<String>, )
- pub fn log_tool_result( state: &mut AgentState, output: impl Into<String>, )
- pub fn log_agent_output( state: &mut AgentState, text: &str, )

**Calls**
- String::new

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
- Cli::parse
- Command::new
- ContextEvent::Error
- CrosstermBackend::new
- Duration::from_millis
- Instant::now
- LogBuffer::new
- Rect::default
- String::from_utf8_lossy
- String::new
- Terminal::new
- Value::Object
- Vec::new
- commands::handle_command
- commands::update_command_hints
- context::spawn_indexer
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
- Path::new
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

### src/ui/diff.rs

**Functions**
-  pub fn from_texts(file: String, before: &str, after: &str) -> Self
- pub fn choose_diff_view(changed: usize) -> DiffView
- pub fn render_diff(diff: &Diff, width: u16) -> Vec<Line<'static>>
- fn diff_strings(before: &str, after: &str) -> Vec<DiffLine>
- fn render_unified(hunks: &[DiffLine]) -> Vec<Line<'static>>
- fn render_side_by_side(hunks: &[DiffLine], width: u16) -> Vec<Line<'static>>
- fn pad_or_trim(s: &str, max: usize) -> String

**Calls**
- Color::Rgb
- DiffLine::Added
- DiffLine::Context
- DiffLine::Removed
- Line::from
- Span::raw
- Span::styled
- Style::default
- Vec::new

### src/ui/helper.rs

**Functions**
- pub fn render_static_command_line( text: &str, width: usize, ) -> Vec<Line<'static>>
- pub fn running_pulse(start: Option<Instant>) -> Option<String>
- pub fn calculate_input_lines(input: &str, width: usize, prompt_len: usize) -> usize

**Calls**
- Color::Rgb
- Command::new
- Line::from
- Span::styled
- String::from_utf8
- String::with_capacity
- Style::default

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

### src/ui/markdown.rs

**Functions**
-  pub fn new() -> Self
-  pub fn render_line(&mut self, input: &str) -> Line<'static>
- fn render_inline_owned(text: &str) -> Line<'static>
- fn take_until<I>(it: &mut std::iter::Peekable<I>, end: &str) -> String where I: Iterator<Item = char>,

**Calls**
- Color::Rgb
- Line::from
- Span::styled
- String::new
- Style::default
- Vec::new

### src/ui/mod.rs

**Functions**

**Calls**

### src/ui/tui.rs

**Functions**
- pub fn draw_ui<B: Backend>( terminal: &mut Terminal<B>, state: &AgentState, ) -> io::Result<(Rect, Rect, Rect)>
- fn render_header(f: &mut Frame, area: Rect, state: &AgentState)
- fn render_execution(f: &mut Frame, area: Rect, state: &AgentState)
- fn render_input_box(f: &mut Frame, area: Rect, state: &AgentState)
- fn render_status_bar(f: &mut Frame, area: Rect, state: &AgentState)
- pub fn render_command_palette( f: &mut Frame, area: Rect, state: &AgentState, )

**Calls**
- Block::default
- Color::Rgb
- Diff::from_texts
- Line::from
- Markdown::new
- Paragraph::new
- Rect::default
- Span::raw
- Span::styled
- String::new
- Style::default
- Vec::new
- diff::render_diff
- dirs::home_dir

## Global Hotspots

### Thread creation
- src/agent.rs → thread::spawn
- src/context/mod.rs → thread::spawn

### Process execution
- src/agent.rs → Command::new
- src/main.rs → Command::new
- src/tools/search.rs → Command::new
- src/tools/shell.rs → Command::new
- src/ui/helper.rs → Command::new

### AgentEvent fan-out
- src/agent.rs
- src/main.rs

