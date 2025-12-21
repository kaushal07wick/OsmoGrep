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
- fn show_test_artifact(state: &mut AgentState)
- fn close_view(state: &mut AgentState)
- pub fn update_command_hints(state: &mut AgentState)

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

### src/detectors/ast/mod.rs

**Functions**

### src/detectors/ast/symboldelta.rs

**Functions**
- pub fn compute_symbol_delta( baseline: DiffBaseline, base_branch: &str, file: &str, symbol: &str, ) -> Option<SymbolDelta>

### src/detectors/diff_analyzer.rs

**Functions**
- pub fn analyze_diff() -> Vec<DiffAnalysis>
- fn analyze_file( base_branch: &str, file: &str, hunks: &str, ) -> DiffAnalysis
- fn split_diff_by_file(diff: &str) -> Vec<(String, String)>
- fn detect_surface(file: &str, hunks: &str) -> ChangeSurface
- fn should_analyze(file: &str) -> bool

### src/detectors/framework.rs

**Functions**
- pub fn detect_framework(root: &Path) -> TestFramework
- fn package_uses_jest(root: &Path) -> bool

### src/detectors/language.rs

**Functions**
- pub fn detect_language(root: &Path) -> Language
- fn dominant_language( py: usize, js: usize, ts: usize, rs: usize, go: usize, java: usize, ) -> Language
- fn is_ignored(path: &Path) -> bool

### src/detectors/mod.rs

**Functions**

### src/executor/mod.rs

**Functions**

### src/executor/run.rs

**Functions**
- pub fn run_single_test(cmd: &[&str]) -> TestResult

### src/git.rs

**Functions**
- pub fn find_existing_agent() -> Option<String>
- pub fn create_agent_branch() -> String
- pub fn detect_base_branch() -> String
- pub fn show_head(path: &str) -> Option<String>
- pub fn show_index(path: &str) -> Option<String>
- pub fn base_commit(base_branch: &str) -> Option<String>
- pub fn show_file_at(commit: &str, path: &str) -> Option<String>

### src/llm/mod.rs

**Functions**

### src/llm/ollama.rs

**Functions**
-  pub fn run(prompt: LlmPrompt) -> io::Result<String>

### src/llm/orchestrator.rs

**Functions**
- pub fn run_llm_test_flow( tx: Sender<AgentEvent>, cancel_flag: Arc<AtomicBool>, language: Language, framework: Option<TestFramework>, candidate: TestCandidate, )

### src/llm/prompt.rs

**Functions**
- pub fn build_prompt( candidate: &TestCandidate, resolution: &TestResolution, language: Option<&Language>, framework: Option<&TestFramework>, ) -> LlmPrompt
- fn user_prompt( c: &TestCandidate, resolution: &TestResolution, language: Option<&Language>, framework: Option<&TestFramework>, ) -> String
- fn format_target(t: &TestTarget) -> String
- fn risk_constraints(risk: &RiskLevel) -> String
- fn test_type_constraints(tt: &TestType) -> String

### src/llm_py/ollama.py

**Functions**
- def main()

### src/logger.rs

**Functions**
- pub fn log( state: &mut AgentState, level: LogLevel, msg: impl Into<String>, )
- pub fn log_diff_analysis(state: &mut AgentState)
-  fn as_str(&self) -> &'static str

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
- fn attach_summaries(state: &mut AgentState)

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

### src/state.rs

**Functions**
-  pub fn new() -> Self
-  pub fn push(&mut self, level: LogLevel, text: impl Into<String>)
-  pub fn iter(&self) -> impl Iterator<Item = &LogLine>
-  pub fn history_prev(&mut self)
-  pub fn history_next(&mut self)
-  pub fn commit_input(&mut self) -> String
-  pub fn update_spinner(&mut self)

### src/testgen/candidate.rs

**Functions**
-  pub fn compute_id( file: &str, symbol: &Option<String>, decision: &TestDecision, ) -> String

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

### src/testgen/generator.rs

**Functions**
- fn normalize_source(file: &str, src: &str) -> String
- fn normalize_rust(src: &str) -> String
- fn normalize_python(src: &str) -> String
- fn is_ui_or_glue_code(d: &DiffAnalysis) -> bool
- fn is_test_worthy(d: &DiffAnalysis) -> bool
- fn decide_test(d: &DiffAnalysis) -> TestDecision
- fn priority(d: &DiffAnalysis) -> u8
- pub fn generate_test_candidates( diffs: &[DiffAnalysis], resolve: impl Fn(&TestCandidate) -> TestResolution, ) -> Vec<TestCandidate>

### src/testgen/mod.rs

**Functions**

### src/testgen/resolve.rs

**Functions**
- pub fn resolve_test( language: &Language, c: &TestCandidate, ) -> TestResolution
- fn resolve_rust_test(c: &TestCandidate) -> TestResolution
- fn resolve_python_test(c: &TestCandidate) -> TestResolution

### src/testgen/summarizer.rs

**Functions**
- pub fn summarize(diff: &DiffAnalysis) -> SemanticSummary
- fn behavior_statement(d: &DiffAnalysis) -> String
- fn failure_statement(d: &DiffAnalysis) -> String
- fn infer_risk(d: &DiffAnalysis) -> RiskLevel

### src/ui/diff.rs

**Functions**
- pub fn render_side_by_side( f: &mut ratatui::Frame, area: Rect, delta: &SymbolDelta, state: &AgentState, )

### src/ui/draw.rs

**Functions**
- pub fn draw_ui<B: Backend>( terminal: &mut Terminal<B>, state: &AgentState, ) -> io::Result<(Rect, Rect, Rect)>

### src/ui/execution.rs

**Functions**
- fn parse_change_line(s: &str) -> Option<(String, String, Option<String>)>
- pub fn render_execution( f: &mut ratatui::Frame, area: Rect, state: &AgentState, )

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

### src/ui/mod.rs

**Functions**

### src/ui/panels.rs

**Functions**
- pub fn render_panel( f: &mut ratatui::Frame, area: Rect, state: &AgentState, ) -> bool
- fn render_testgen_panel( f: &mut ratatui::Frame, area: Rect, state: &AgentState, candidate: &crate::testgen::candidate::TestCandidate, generated_test: &Option<String>, )
- fn render_test_result_panel( f: &mut ratatui::Frame, area: Rect, state: &AgentState, output: &str, passed: bool, )
- fn render_scrollable_block( f: &mut ratatui::Frame, area: Rect, scroll: usize, lines: Vec<Line>, title: &str, )

### src/ui/status.rs

**Functions**
- pub fn render_status( f: &mut ratatui::Frame, area: Rect, state: &AgentState, )
- fn render_header(f: &mut ratatui::Frame, area: Rect)
- fn render_status_block( f: &mut ratatui::Frame, area: Rect, state: &AgentState, )

