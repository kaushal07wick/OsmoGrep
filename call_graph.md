# Osmogrep Agent Map

> Auto-generated. Do not edit manually.

## Files

### src/commands.rs

**Functions**
- pub fn handle_command(state: &mut AgentState, cmd: &str)
- fn inspect(state: &mut AgentState)
- fn list_changes(state: &mut AgentState)
- fn view_change(state: &mut AgentState, cmd: &str)
- fn agent_run(state: &mut AgentState, cmd: &str)
- fn agent_status(state: &mut AgentState)
- fn agent_cancel(state: &mut AgentState)
- fn model_use(state: &mut AgentState, cmd: &str)
- fn enter_api_key_mode( state: &mut AgentState, provider: Provider, model: &str, )
- fn model_show(state: &mut AgentState)
- fn show_test_artifact(state: &mut AgentState)
- fn close_view(state: &mut AgentState)
- pub fn update_command_hints(state: &mut AgentState)

**Calls**
- Instant::now
- LlmBackend::ollama
- env::current_dir
- git::list_branches
- slice::from_ref

### src/context/mod.rs

**Functions**

**Calls**

### src/context/snapshot.rs

**Functions**
- pub fn build_full_context_snapshot( repo_root: &Path, diffs: &[DiffAnalysis], ) -> FullContextSnapshot
- pub fn build_context_snapshot( repo_root: &Path, diffs: &[DiffAnalysis], ) -> ContextSnapshot
- pub fn parse_file( file: &Path, source: &str, ) -> (Vec<SymbolDef>, Vec<Import>)
- fn walk_node( kind: LanguageKind, node: Node, file: &Path, src: &str, symbols: &mut Vec<SymbolDef>, imports: &mut Vec<Import>, current_class: Option<String>, )
- fn resolve_symbol( target: Option<&str>, symbols: &[SymbolDef], ) -> SymbolResolution

**Calls**
- Parser::new
- PathBuf::from
- SymbolResolution::Ambiguous
- SymbolResolution::Resolved
- Vec::new
- fs::read_to_string
- python::language
- rust::language

### src/context/test_snapshot.rs

**Functions**
- pub fn build_test_context_snapshot(repo_root: &Path) -> TestContextSnapshot
- fn empty_snapshot(exists: bool, test_roots: Vec<PathBuf>) -> TestContextSnapshot
- fn collect_test_files_recursive(test_roots: &[PathBuf]) -> Vec<PathBuf>
- fn collect_py_files(dir: &Path, out: &mut Vec<PathBuf>)
- fn detect_framework_from_source( source: &str, symbols: &[SymbolDef], ) -> TestFramework
- fn detect_references(imports: &[Import]) -> Vec<String>
- fn detect_style( helpers: &[String], references: &[String], symbols: &[SymbolDef], ) -> Option<TestStyle>

**Calls**
- Vec::new
- fs::read_dir
- fs::read_to_string

### src/context/types.rs

**Functions**

**Calls**

### src/detectors/ast/ast.rs

**Functions**
- pub fn detect_symbol( source: &str, hunks: &str, file: &str, ) -> Option<String>
- pub fn extract_symbol_source( source: &str, file: &str, symbol: &str, ) -> Option<String>
- pub fn parse_source(file: &str, source: &str) -> Option<tree_sitter::Tree>
- pub fn compute_line_offsets(src: &str) -> Vec<usize>
- pub fn changed_byte_ranges( hunks: &str, offsets: &[usize], ) -> Vec<(usize, usize)>
- pub fn parse_hunk_header(line: &str) -> Option<(usize, usize)>
- fn collect_enclosing_symbols( node: Node, source: &str, start: usize, end: usize, best: &mut Option<(usize, String)>, )
- fn symbol_name(node: Node, source: &str) -> Option<String>
- pub fn find_symbol_node<'a>( node: Node<'a>, source: &str, symbol: &str, ) -> Option<Node<'a>>

**Calls**
- Parser::new
- RefCell::new
- Vec::new
- tree_sitter_python::language
- tree_sitter_rust::language

### src/detectors/ast/mod.rs

**Functions**

**Calls**

### src/detectors/ast/symboldelta.rs

**Functions**
- pub fn compute_symbol_delta( old_source: &str, new_source: &str, file: &str, symbol: &str, ) -> Option<SymbolDelta>

**Calls**

### src/detectors/diff_analyzer.rs

**Functions**
- pub fn analyze_diff() -> Vec<DiffAnalysis>
- fn analyze_file( base_branch: &str, file: &str, hunks: &str, ) -> Option<DiffAnalysis>
- fn split_diff_by_file(diff: &str) -> Vec<(String, String)>
- fn detect_surface(file: &str, hunks: &str) -> ChangeSurface
- fn should_analyze(file: &str) -> bool

**Calls**
- String::from_utf8_lossy
- String::new
- Vec::new
- git::base_commit
- git::detect_base_branch
- git::diff_cached
- git::show_file_at
- git::show_head
- git::show_index
- mem::take

### src/detectors/framework.rs

**Functions**
- pub fn detect_framework(root: &Path) -> TestFramework
- fn package_uses_jest(root: &Path) -> bool
-  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result

**Calls**
- fs::read_to_string

### src/detectors/language.rs

**Functions**
- pub fn detect_language(root: &Path) -> Language
- fn dominant_language( py: usize, js: usize, ts: usize, rs: usize, go: usize, java: usize, ) -> Language
- fn is_ignored(path: &Path) -> bool
-  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result

**Calls**
- WalkDir::new

### src/detectors/mod.rs

**Functions**

**Calls**

### src/git.rs

**Functions**
- pub fn find_existing_agent() -> Option<String>
- pub fn create_agent_branch() -> String
- pub fn detect_base_branch() -> String
- pub fn show_head(path: &str) -> Option<String>
- pub fn show_index(path: &str) -> Option<String>
- pub fn base_commit(base_branch: &str) -> Option<String>
- pub fn show_file_at(commit: &str, path: &str) -> Option<String>

**Calls**
- Command::new
- Path::new
- String::from_utf8_lossy

### src/llm/backend.rs

**Functions**
-  pub fn ollama(model: String) -> Self
-  pub fn remote(client: LlmClient) -> Self
-  pub fn run(&self, prompt: LlmPrompt) -> Result<LlmRunResult, String>

**Calls**
- Ollama::run

### src/llm/client.rs

**Functions**
-  pub fn new() -> Self
-  pub fn configure( &self, provider_name: &str, model: String, api_key: String, base_url: Option<String>, ) -> Result<(), String>
-  pub fn run(&self, prompt: LlmPrompt) -> Result<LlmRunResult, String>
- fn build_request( cfg: &ProviderConfig, prompt: &LlmPrompt, prompt_hash: &str, ) -> (String, Vec<(&'static str, String)>, Value)
- fn extract_text(provider: &Provider, v: &Value) -> Result<String, String>
- fn default_config() -> ProviderConfig
- fn save_config(cfg: &ProviderConfig) -> std::io::Result<()>

**Calls**
- Arc::new
- Client::builder
- Duration::from_secs
- Mutex::new
- PathBuf::from
- Sha256::new
- String::new
- dirs::config_dir
- fs::create_dir_all
- fs::read_to_string
- fs::write
- hex::encode
- serde_json::from_str
- serde_json::to_string_pretty

### src/llm/mod.rs

**Functions**

**Calls**

### src/llm/ollama.rs

**Functions**
-  pub fn run(prompt: LlmPrompt, model: &str) -> io::Result<String>
- fn ollama_script_path() -> io::Result<PathBuf>
- fn python_bin() -> &'static str
- fn wait_with_timeout( mut child: std::process::Child, timeout: Duration, ) -> io::Result<std::process::Output>

**Calls**
- Command::new
- Duration::from_millis
- Duration::from_secs
- Error::new
- Instant::now
- PathBuf::from
- Stdio::piped
- String::from_utf8_lossy
- thread::sleep

### src/llm/orchestrator.rs

**Functions**
- pub fn run_llm_test_flow( tx: Sender<AgentEvent>, cancel_flag: Arc<AtomicBool>, llm: LlmBackend, snapshot: FullContextSnapshot, candidate: TestCandidate, language: Language, semantic_cache: Arc<SemanticCache>, )
- fn trim_error(s: &str) -> String
- fn sanitize_llm_output(raw: &str) -> String

**Calls**
- AgentEvent::Failed
- AgentEvent::GeneratedTest
- AgentEvent::Log
- AgentEvent::TestFinished
- SemanticKey::from_candidate
- env::current_dir
- thread::spawn

### src/llm/prompt.rs

**Functions**
- pub fn build_prompt( candidate: &TestCandidate, file_ctx: &FileContext, test_ctx: &TestContextSnapshot, ) -> LlmPrompt
- pub fn build_prompt_with_feedback( candidate: &TestCandidate, file_ctx: &FileContext, test_ctx: &TestContextSnapshot, previous_test: &str, failure_feedback: &str, ) -> LlmPrompt
- fn user_prompt( candidate: &TestCandidate, ctx: &FileContext, test_ctx: &TestContextSnapshot, ) -> String

**Calls**
- String::new
- SymbolResolution::Ambiguous
- SymbolResolution::Resolved

### src/llm_py/ollama.py

**Functions**
- def main()

**Calls**
- environ.get
- prompt.strip
- stderr.write
- stdin.read
- stdout.write
- subprocess.run
- sys.exit

### src/logger.rs

**Functions**
- pub fn log( state: &mut AgentState, level: LogLevel, msg: impl Into<String>, )
- pub fn log_diff_analysis(state: &mut AgentState)
-  fn as_str(&self) -> &'static str

**Calls**

### src/machine.rs

**Functions**
- pub fn step(state: &mut AgentState)
- fn init_repo(state: &mut AgentState)
- fn detect_base_branch(state: &mut AgentState)
- fn create_agent_branch(state: &mut AgentState)
- fn execute_agent(state: &mut AgentState)
- fn handle_running(state: &mut AgentState)
- fn rollback_agent(state: &mut AgentState)
- fn ensure_agent_branch(state: &mut AgentState) -> String
- fn return_to_base_branch(state: &mut AgentState)
- fn assert_repo_root() -> Result<PathBuf, String>
- fn attach_summaries(state: &mut AgentState)

**Calls**
- Arc::new
- SemanticCache::new
- env::current_dir
- git::checkout
- git::create_agent_branch
- git::current_branch
- git::delete_branch
- git::detect_base_branch
- git::find_existing_agent
- git::is_git_repo
- git::working_tree_dirty

### src/main.rs

**Functions**
- fn main() -> Result<(), Box<dyn Error>>
- fn drain_agent_events(state: &mut AgentState)
- fn init_state() -> AgentState
- fn handle_event( state: &mut AgentState, event: impl Into<Event>, input_rect: ratatui::layout::Rect, diff_rect: ratatui::layout::Rect, exec_rect: ratatui::layout::Rect, )
- fn handle_key(state: &mut AgentState, k: crossterm::event::KeyEvent)
- fn handle_input_keys(state: &mut AgentState, k: crossterm::event::KeyEvent)
- fn handle_diff_keys(state: &mut AgentState, k: crossterm::event::KeyEvent)
- fn handle_exec_keys(state: &mut AgentState, k: crossterm::event::KeyEvent)
- fn handle_mouse( state: &mut AgentState, m: crossterm::event::MouseEvent, input_rect: ratatui::layout::Rect, diff_rect: ratatui::layout::Rect, exec_rect: ratatui::layout::Rect, )
- fn teardown_terminal( terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, ) -> Result<(), Box<dyn Error>>

**Calls**
- AgentEvent::Failed
- AgentEvent::GeneratedTest
- AgentEvent::Log
- AgentEvent::SpinnerStart
- AgentEvent::TestFinished
- Arc::new
- AtomicBool::new
- CrosstermBackend::new
- Duration::from_millis
- Event::Key
- Event::Mouse
- Instant::now
- KeyCode::Char
- LlmBackend::remote
- LlmClient::new
- LogBuffer::new
- MouseEventKind::Down
- String::new
- Terminal::new
- Vec::new
- event::poll
- event::read
- io::stdout
- ui::draw_ui

### src/state.rs

**Functions**
-  pub fn new() -> Self
-  pub fn push(&mut self, level: LogLevel, text: impl Into<String>)
-  pub fn iter(&self) -> impl Iterator<Item = &LogLine>
-  pub fn history_prev(&mut self)
-  pub fn history_next(&mut self)
-  pub fn commit_input(&mut self) -> String
-  pub fn on_logs_appended(&mut self)
-  pub fn update_spinner(&mut self)

**Calls**
- Duration::from_secs
- Instant::now
- VecDeque::with_capacity
- logger::log

### src/testgen/cache.rs

**Functions**
-  pub fn from_candidate(c: &TestCandidate) -> Self
-  pub fn to_cache_key(&self) -> String
-  pub fn new() -> Self

**Calls**
- HashMap::new
- Mutex::new
- Sha256::new
- hex::encode

### src/testgen/candidate.rs

**Functions**

**Calls**

### src/testgen/generator.rs

**Functions**
- fn normalize_source(file: &str, src: &str) -> String
- fn normalize_rust(src: &str) -> String
- fn normalize_python(src: &str) -> String
- fn is_test_worthy(d: &DiffAnalysis) -> bool
- fn decide_test(d: &DiffAnalysis) -> TestDecision
- fn priority(d: &DiffAnalysis) -> u8
- pub fn generate_test_candidates( diffs: &[DiffAnalysis], ) -> Vec<TestCandidate>

**Calls**
- TestTarget::File
- TestTarget::Symbol
- Vec::new
- summarizer::summarize

### src/testgen/materialize.rs

**Functions**
- pub fn materialize_test( repo_root: &Path, language: Language, candidate: &TestCandidate, test_code: &str, ) -> io::Result<PathBuf>
- fn write_python_test( repo_root: &Path, candidate: &TestCandidate, test_code: &str, ) -> io::Result<PathBuf>
- fn write_rust_test( repo_root: &Path, candidate: &TestCandidate, test_code: &str, ) -> io::Result<PathBuf>
- fn find_test_root(repo_root: &Path) -> io::Result<PathBuf>
- fn ensure_parent_dir(path: &Path) -> io::Result<()>
- fn sanitize_name(file: &str, symbol: &Option<String>) -> String

**Calls**
- Error::new
- File::create
- Path::new
- fs::create_dir_all

### src/testgen/mod.rs

**Functions**

**Calls**

### src/testgen/runner.rs

**Functions**
- pub fn run_test(req: TestRunRequest) -> TestResult
- pub fn run_full_test_async<F>(language: Language, on_done: F) where F: FnOnce(TestSuiteResult) + Send + 'static,
- fn run_full_python_tests() -> TestSuiteResult
- fn run_full_rust_tests() -> TestSuiteResult

**Calls**
- Command::new
- Instant::now
- String::from_utf8_lossy
- String::new
- Vec::new
- env::current_dir
- thread::spawn

### src/testgen/summarizer.rs

**Functions**
- pub fn summarize(diff: &DiffAnalysis) -> SemanticSummary
- fn behavior_statement(d: &DiffAnalysis) -> String
- fn failure_statement(d: &DiffAnalysis) -> String
- fn infer_risk(d: &DiffAnalysis) -> RiskLevel

**Calls**

### src/testgen/test_suite.rs

**Functions**
- pub fn run_full_test_suite(state: &AgentState, repo_root: PathBuf) -> io::Result<()>
- fn parse_pytest_output_fully(raw: &str) -> (ParsedSummary, Vec<TestCaseResult>)
- fn parse_footer_summary_line(line: &str, out: &mut ParsedSummary)
- fn extract_failed_tests(raw: &str) -> std::collections::HashSet<String>
- fn extract_failure_spans(raw: &str) -> HashMap<String, FailureSpan>
- fn extract_durations(raw: &str) -> HashMap<String, f64>
- fn extract_skips(raw: &str) -> Vec<SkipEntry>
- fn extract_warnings(raw: &str) -> Vec<WarningEntry>
- fn extract_verbose_failures(raw: &str) -> Vec<(String, String)>
- pub fn write_test_suite_report( repo_root: &Path, suite: &TestSuiteResult, ) -> io::Result<PathBuf>

**Calls**
- AgentEvent::Log
- BTreeMap::new
- Error::new
- File::create
- HashMap::new
- HashSet::new
- ParsedSummary::default
- String::new
- SystemTime::now
- Vec::new
- fs::create_dir_all
- fs::write
- serde_json::to_string_pretty

### src/ui/diff.rs

**Functions**
- pub fn render_side_by_side( f: &mut ratatui::Frame, area: Rect, delta: &SymbolDelta, state: &AgentState, )

**Calls**
- Block::default
- Constraint::Percentage
- Layout::default
- Line::from
- Paragraph::new
- Span::raw
- Span::styled
- Style::default
- TextDiff::from_lines
- Vec::with_capacity

### src/ui/draw.rs

**Functions**
- pub fn draw_ui<B: Backend>( terminal: &mut Terminal<B>, state: &AgentState, ) -> io::Result<(Rect, Rect, Rect)>

**Calls**
- Block::default
- Constraint::Length
- Constraint::Min
- Layout::default
- Line::from
- Paragraph::new
- Rect::default
- Span::styled
- String::new
- Style::default
- UnicodeWidthStr::width
- diff::render_side_by_side
- execution::render_execution
- panels::render_panel
- status::render_status

### src/ui/execution.rs

**Functions**
- fn parse_change_line(s: &str) -> Option<(String, String, Option<String>)>
- pub fn render_execution( f: &mut ratatui::Frame, area: Rect, state: &AgentState, )

**Calls**
- Block::default
- Duration::from_secs
- Instant::now
- Line::from
- Paragraph::new
- Span::raw
- Span::styled
- Style::default
- Vec::new

### src/ui/helpers.rs

**Functions**
- pub fn phase_badge(phase: &Phase) -> (&'static str, &'static str, Color)
- pub fn language_badge(lang: &str) -> (&'static str, Color)
- pub fn framework_badge(fw: &str) -> (&'static str, Color)
- pub fn ln(n: usize, color: Color) -> Span<'static>
- pub fn decision_color(d: &TestDecision) -> Color
- pub fn risk_color(r: &RiskLevel) -> Color
- pub fn hclip(s: &str, x: usize, width: usize) -> &str
- pub fn surface_color(surface: &ChangeSurface) -> Color

**Calls**
- Span::styled
- Style::default

### src/ui/mod.rs

**Functions**

**Calls**

### src/ui/panels.rs

**Functions**
- pub fn render_panel( f: &mut ratatui::Frame, area: Rect, state: &AgentState, ) -> bool
- fn render_testgen_panel( f: &mut ratatui::Frame, area: Rect, state: &AgentState, candidate: &crate::testgen::candidate::TestCandidate, generated_test: &Option<String>, )
- fn render_test_result_panel( f: &mut ratatui::Frame, area: Rect, state: &AgentState, output: &str, passed: bool, )
- fn render_scrollable_block( f: &mut ratatui::Frame, area: Rect, scroll: usize, lines: Vec<Line>, title: &str, )

**Calls**
- Block::default
- Line::from
- Paragraph::new
- Span::raw
- Span::styled
- Style::default
- Vec::new

### src/ui/status.rs

**Functions**
- pub fn render_status( f: &mut ratatui::Frame, area: Rect, state: &AgentState, )
- fn render_header(f: &mut ratatui::Frame, area: Rect)
- fn render_status_block( f: &mut ratatui::Frame, area: Rect, state: &AgentState, )
- fn render_context_block( f: &mut ratatui::Frame, area: Rect, state: &AgentState, )

**Calls**
- Block::default
- Constraint::Length
- Constraint::Percentage
- Layout::default
- Line::from
- Paragraph::new
- Span::raw
- Span::styled
- Style::default
- System::new
- Vec::new

## Global Hotspots

### Thread creation
- src/llm/orchestrator.rs → thread::spawn
- src/testgen/runner.rs → thread::spawn

### Process execution
- src/git.rs → Command::new
- src/llm/ollama.rs → Command::new
- src/testgen/runner.rs → Command::new

### AgentEvent fan-out
- src/llm/orchestrator.rs
- src/main.rs
- src/testgen/test_suite.rs

