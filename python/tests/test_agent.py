from pathlib import Path

from weaveback_agent import AgentLoop, WorkspaceConfig
from weaveback_agent._weaveback import PyWorkspace


def test_pyworkspace_basic(tmp_path: Path) -> None:
    db_path = tmp_path / "wb.db"
    gen_dir = tmp_path / "gen"
    gen_dir.mkdir()

    ws = PyWorkspace(str(tmp_path), str(db_path), str(gen_dir))

    search = ws.search("test", 10)
    assert isinstance(search, list)

    trace = ws.trace("nonexistent.rs", 1, 1)
    assert trace is None or isinstance(trace, dict)


def test_agent_loop_trace_and_summary(tmp_path: Path) -> None:
    db_path = tmp_path / "wb.db"
    gen_dir = tmp_path / "gen"
    gen_dir.mkdir()

    loop = AgentLoop(
        WorkspaceConfig(
            project_root=str(tmp_path),
            db_path=str(db_path),
            gen_dir=str(gen_dir),
        )
    )

    assert loop.inspect_trace("nonexistent.rs", 1, 1) is None

    response = loop.run_once("do nothing", planner=object())
    assert "Planner integration belongs here" in response.summary
    assert response.plan is None
    assert response.preview is None
