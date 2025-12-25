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
- fn set_model(state: &mut AgentState, cmd: &str)
- fn show_model(state: &mut AgentState)
- fn status_model(state: &mut AgentState)
- fn status_agent(state: &mut AgentState)
- fn status_context(state: &mut AgentState)
- fn status_prompt(state: &mut AgentState)
- fn status_git(state: &mut AgentState)
- fn status_system(state: &mut AgentState)
- fn agent_cancel(state: &mut AgentState)
- fn show_test_artifact(state: &mut AgentState)
- fn close_view(state: &mut AgentState)
- pub fn update_command_hints(state: &mut AgentState)

**Calls**
- IndexStatus::Failed
- Instant::now
- fs::read_to_string
- git::current_branch
- git::list_branches
- git::working_tree_dirty
- slice::from_ref

### src/context/engine.rs

**Functions**
-  pub fn new( repo_root: &'a Path, facts: &'a RepoFacts, _symbols: &'a super::types::SymbolIndex, // intentionally ignored test_roots: &'a [PathBuf], ) -> Self
-  pub fn slice_from_diff(&self, diff: &DiffAnalysis) -> ContextSlice
-  fn build_test_context( &self, src_file: &Path, symbol: Option<&str>, ) -> TestContext
- fn parse_file( file: &Path, source: &str, ) -> (Vec<SymbolDef>, Vec<Import>)
- fn walk( node: Node, file: &Path, src: &str, symbols: &mut Vec<SymbolDef>, imports: &mut Vec<Import>, current_class: Option<String>, )
- fn resolve_symbol( target: Option<&str>, symbols: &[SymbolDef], ) -> SymbolResolution
- fn find_candidate_tests( test_roots: &[PathBuf], src_file: &Path, ) -> Vec<PathBuf>
- fn match_symbol_in_tests( files: &[PathBuf], symbol: &str, ) -> Vec<PathBuf>
- fn symbol_variants(symbol: &str) -> Vec<String>
- fn default_test_path( repo_root: &Path, src_file: &Path, symbol: Option<&str>, ) -> PathBuf

**Calls**
- Parser::new
- PathBuf::from
- RepoFactsLite::from
- SymbolResolution::Ambiguous
- SymbolResolution::Resolved
- Vec::new
- WalkDir::new
- fs::read_to_string
- python::language
- rust::language

### src/context/index.rs

**Functions**
- pub fn spawn_repo_indexer(repo_root: PathBuf) -> IndexHandle
- fn extract_repo_facts(repo_root: &Path) -> RepoFacts
- fn detect_code_roots(repo_root: &Path) -> Vec<PathBuf>
- fn build_symbol_index( repo_root: &Path, code_roots: &[PathBuf], test_roots: &[PathBuf], ) -> SymbolIndex
- fn index_file( abs_path: &Path, rel_path: &Path, kind: LanguageKind, ) -> Option<FileSymbols>
- fn walk_node( kind: LanguageKind, node: Node, file: &Path, src: &str, symbols: &mut Vec<SymbolDef>, imports: &mut Vec<Import>, current_class: Option<String>, )

**Calls**
- HashMap::new
- IndexHandle::new_indexing
- Parser::new
- Vec::new
- WalkDir::new
- fs::read_to_string
- python::language
- rust::language
- thread::spawn

### src/context/mod.rs

**Functions**

**Calls**

### src/context/types.rs

**Functions**
-  pub fn from(facts: &RepoFacts) -> Self
-  pub fn new_indexing(repo_root: PathBuf) -> Self

**Calls**
- Arc::new
- IndexStatus::Failed
- RwLock::new
- Vec::new

### src/detectors/ast/ast.rs

**Functions**
- pub fn detect_symbol(file: &str, hunks: &str) -> Option<String>
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
- fs::read_to_string
- git::show_head
- git::show_index
- tree_sitter_python::language
- tree_sitter_rust::language

### src/detectors/ast/mod.rs

**Functions**

**Calls**

### src/detectors/ast/symboldelta.rs

**Functions**
- pub fn compute_symbol_delta( baseline: DiffBaseline, base_branch: &str, file: &str, symbol: &str, ) -> Option<SymbolDelta>

**Calls**
- git::base_commit
- git::show_file_at
- git::show_head
- git::show_index

### src/detectors/diff_analyzer.rs

**Functions**
- pub fn analyze_diff() -> Vec<DiffAnalysis>
- fn analyze_file( base_branch: &str, file: &str, hunks: &str, ) -> DiffAnalysis
- fn split_diff_by_file(diff: &str) -> Vec<(String, String)>
- fn detect_surface(file: &str, hunks: &str) -> ChangeSurface
- fn should_analyze(file: &str) -> bool

**Calls**
- String::from_utf8_lossy
- String::new
- Vec::new
- git::detect_base_branch
- git::diff_cached
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

### src/executor/mod.rs

**Functions**

**Calls**

### src/executor/run.rs

**Functions**
- pub fn run_single_test(cmd: &[&str]) -> TestResult

**Calls**
- Command::new
- String::from_utf8_lossy
- String::new

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

### src/llm/mod.rs

**Functions**

**Calls**

### src/llm/ollama.rs

**Functions**
-  pub fn run(prompt: LlmPrompt, model: &str) -> io::Result<String>

**Calls**
- Command::new
- Error::new
- PathBuf::from
- Stdio::piped
- String::from_utf8_lossy

### src/llm/orchestrator.rs

**Functions**
- pub fn run_llm_test_flow( tx: Sender<AgentEvent>, cancel_flag: Arc<AtomicBool>, context_index: IndexHandle, candidate: TestCandidate, model: String, )
- fn debug_context(ctx: &ContextSlice) -> String

**Calls**
- AgentEvent::Failed
- AgentEvent::Log
- ContextEngine::new
- Duration::from_millis
- IndexStatus::Failed
- String::new
- SymbolResolution::Ambiguous
- SymbolResolution::Resolved
- fs::write
- thread::sleep
- thread::spawn

### src/llm/prompt.rs

**Functions**
- pub fn build_prompt( candidate: &TestCandidate, resolution: &TestResolution, context: &ContextSlice, ) -> LlmPrompt
- fn user_prompt( c: &TestCandidate, _resolution: &TestResolution, ctx: &ContextSlice, ) -> String

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
- context::spawn_repo_indexer
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

### src/testgen/candidate.rs

**Functions**

**Calls**

### src/testgen/file.rs

**Functions**
- pub fn materialize_test( language: Language, candidate: &TestCandidate, resolution: &TestResolution, test_code: &str, ) -> io::Result<PathBuf>
- fn write_python_test( candidate: &TestCandidate, resolution: &TestResolution, test_code: &str, ) -> io::Result<PathBuf>
- fn default_python_test_path(c: &TestCandidate) -> PathBuf
- fn write_rust_test( candidate: &TestCandidate, resolution: &TestResolution, test_code: &str, ) -> io::Result<PathBuf>
- fn default_rust_test_path(c: &TestCandidate) -> PathBuf
- fn ensure_parent_dir(path: &Path) -> io::Result<()>
- fn append_once( path: &Path, content: &str, sentinel: &str, ) -> io::Result<()>
- fn append_inline_rust_test( path: &Path, test_code: &str, ) -> io::Result<()>
- fn indent(s: &str, spaces: usize) -> String
- fn sanitize(file: &str, symbol: &Option<String>) -> String

**Calls**
- Error::new
- OpenOptions::new
- PathBuf::from
- TestResolution::Ambiguous
- fs::create_dir_all
- fs::read_to_string

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
- String::new
- TestTarget::File
- Vec::new
- cmp::Reverse
- summarizer::summarize

### src/testgen/mod.rs

**Functions**

**Calls**

### src/testgen/resolve.rs

**Functions**
- pub fn resolve_test( c: &TestCandidate, ctx: Option<&TestContext>, ) -> TestResolution
- fn resolve_from_context( c: &TestCandidate, ctx: &TestContext, ) -> TestResolution

**Calls**
- TestResolution::Ambiguous
- Vec::new
- fs::read_to_string

### src/testgen/summarizer.rs

**Functions**
- pub fn summarize(diff: &DiffAnalysis) -> SemanticSummary
- fn behavior_statement(d: &DiffAnalysis) -> String
- fn failure_statement(d: &DiffAnalysis) -> String
- fn infer_risk(d: &DiffAnalysis) -> RiskLevel

**Calls**

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
- Style::default
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
- IndexStatus::Failed
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
- src/context/index.rs → thread::spawn
- src/llm/orchestrator.rs → thread::spawn

### Process execution
- src/executor/run.rs → Command::new
- src/git.rs → Command::new
- src/llm/ollama.rs → Command::new

### AgentEvent fan-out
- src/llm/orchestrator.rs
- src/main.rs

