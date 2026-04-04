from typing import Annotated, Self

from pydantic import BaseModel, ConfigDict, Field, StringConstraints, model_validator

StrictModelConfig = ConfigDict(extra="forbid", strict=True, validate_assignment=True)

NonEmptyStr = Annotated[str, StringConstraints(strip_whitespace=True, min_length=1)]
SourceLine = Annotated[int, Field(ge=1)]
OutputLine = Annotated[int, Field(ge=1)]
SourceLines = Annotated[list[str], Field(min_length=1)]
IssueList = Annotated[list[str], Field(default_factory=list)]


class StrictModel(BaseModel):
    model_config = StrictModelConfig


class WorkspaceConfig(StrictModel):
    project_root: NonEmptyStr
    db_path: NonEmptyStr = "weaveback.db"
    gen_dir: NonEmptyStr = "gen"


class ChangeTarget(StrictModel):
    src_file: NonEmptyStr
    src_line: SourceLine
    src_line_end: SourceLine

    @model_validator(mode="after")
    def validate_line_range(self) -> Self:
        if self.src_line_end < self.src_line:
            msg = "src_line_end must be greater than or equal to src_line"
            raise ValueError(msg)
        return self


class OutputAnchor(StrictModel):
    out_file: NonEmptyStr
    out_line: OutputLine
    expected_output: str


class PlannedEdit(StrictModel):
    edit_id: NonEmptyStr
    rationale: NonEmptyStr
    target: ChangeTarget
    new_src_lines: SourceLines
    anchor: OutputAnchor


class ChangePlan(StrictModel):
    plan_id: NonEmptyStr
    goal: NonEmptyStr
    constraints: list[NonEmptyStr] = Field(default_factory=list)
    edits: Annotated[list[PlannedEdit], Field(min_length=1)]

    @model_validator(mode="after")
    def validate_edit_ids(self) -> Self:
        edit_ids = [edit.edit_id for edit in self.edits]
        if len(edit_ids) != len(set(edit_ids)):
            msg = "edit_id values must be unique within a ChangePlan"
            raise ValueError(msg)
        return self


class PlanValidation(StrictModel):
    ok: bool
    issues: IssueList


class PreviewedEdit(StrictModel):
    edit_id: NonEmptyStr
    oracle_ok: bool
    src_before: SourceLines
    src_after: SourceLines


class ChangePreview(StrictModel):
    plan_id: NonEmptyStr
    edits: Annotated[list[PreviewedEdit], Field(min_length=1)]


class ApplyResult(StrictModel):
    plan_id: NonEmptyStr
    applied: bool
    applied_edit_ids: list[NonEmptyStr]
    failed_edit_ids: list[NonEmptyStr]


class TraceResult(StrictModel):
    out_file: NonEmptyStr
    out_line: OutputLine
    src_file: NonEmptyStr | None = None
    src_line: SourceLine | None = None
    src_col: SourceLine | None = None
    kind: NonEmptyStr | None = None
    macro_name: NonEmptyStr | None = None
    param_name: NonEmptyStr | None = None
