from typing import Annotated

from pydantic import Field

from weaveback_agent._weaveback import PyWorkspace
from weaveback_agent.models import (
    ChangePlan,
    ChangePreview,
    PlanValidation,
    StrictModel,
    TraceResult,
    WorkspaceConfig,
)


class AgentResponse(StrictModel):
    summary: Annotated[str, Field(min_length=1)]
    plan: ChangePlan | None = None
    validation: PlanValidation | None = None
    preview: ChangePreview | None = None
    traces: list[TraceResult] = Field(default_factory=list)


class AgentLoop:
    def __init__(self, config: WorkspaceConfig) -> None:
        self._workspace = PyWorkspace(
            project_root=config.project_root,
            db_path=config.db_path,
            gen_dir=config.gen_dir,
        )

    def inspect_trace(self, out_file: str, out_line: int, out_col: int = 1) -> TraceResult | None:
        raw = self._workspace.trace(out_file, out_line, out_col)
        if raw is None:
            return None
        return TraceResult.model_validate(raw)

    def validate_plan(self, plan: ChangePlan) -> PlanValidation:
        raw = self._workspace.validate_change_plan(plan.model_dump(mode="python"))
        return PlanValidation.model_validate(raw)

    def preview_plan(self, plan: ChangePlan) -> ChangePreview:
        raw = self._workspace.preview_change_plan(plan.model_dump(mode="python"))
        return ChangePreview.model_validate(raw)

    def run_once(self, task: str, planner: object) -> AgentResponse:
        del task
        del planner
        return AgentResponse(
            summary="Planner integration belongs here; the Rust boundary stays narrow.",
        )
