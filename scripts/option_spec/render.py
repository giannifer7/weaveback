"""Render a small CLI option-spec into several projections.

The schema is intentionally narrow. The goal is to validate the "define once,
project everywhere" shape before touching the real weaveback CLI.
"""

import argparse
import json
from dataclasses import dataclass
from pathlib import Path
import tomllib


type ValueKind = str


@dataclass(slots=True, frozen=True)
class OptionSpec:
    command: str
    id: str
    long: str
    short: str | None
    value_kind: ValueKind
    default: str | bool | int | None
    help_short: str
    rationale: str
    examples: tuple[str, ...]
    doc: str | None = None
    required: bool = False
    repeatable: bool = False

    def validate(self) -> None:
        if not self.id:
            raise ValueError("option id must not be empty")
        if not self.long:
            raise ValueError(f"option {self.id} must declare a long name")
        if self.value_kind not in {"bool", "string", "path", "int"}:
            raise ValueError(
                f"option {self.id} uses unsupported value_kind {self.value_kind!r}"
            )
        if self.short and len(self.short) != 1:
            raise ValueError(f"option {self.id} short name must be one character")
        if self.value_kind == "bool" and self.repeatable:
            raise ValueError(f"boolean flag {self.id} must not be repeatable")


@dataclass(slots=True, frozen=True)
class CommandSpec:
    name: str
    summary: str
    rationale: str
    examples: tuple[str, ...]
    options: tuple[OptionSpec, ...]

    def validate(self) -> None:
        if not self.name:
            raise ValueError("command name must not be empty")
        if not self.options:
            raise ValueError(f"command {self.name} must declare at least one option")
        seen: set[str] = set()
        for option in self.options:
            option.validate()
            if option.command != self.name:
                raise ValueError(
                    f"option {option.id} belongs to {option.command}, expected {self.name}"
                )
            if option.id in seen:
                raise ValueError(f"duplicate option id {option.id!r}")
            seen.add(option.id)


def load_spec(path: Path) -> CommandSpec:
    raw = tomllib.loads(path.read_text(encoding="utf-8"))
    command_raw = raw["command"]
    command_name = str(command_raw["name"])
    options = tuple(
        OptionSpec(
            command=command_name,
            id=str(item["id"]),
            long=str(item["long"]),
            short=str(item["short"]) if item.get("short") else None,
            value_kind=str(item["value_kind"]),
            default=item.get("default"),
            help_short=str(item["help_short"]),
            rationale=str(item["rationale"]),
            examples=tuple(str(example) for example in item.get("examples", [])),
            doc=str(item["doc"]) if item.get("doc") else None,
            required=bool(item.get("required", False)),
            repeatable=bool(item.get("repeatable", False)),
        )
        for item in raw.get("options", [])
    )
    command = CommandSpec(
        name=command_name,
        summary=str(command_raw["summary"]),
        rationale=str(command_raw["rationale"]),
        examples=tuple(str(example) for example in command_raw.get("examples", [])),
        options=options,
    )
    command.validate()
    return command


def rust_type_for(option: OptionSpec) -> str:
    if option.value_kind == "bool":
        return "bool"
    if option.value_kind == "path":
        return "std::path::PathBuf"
    if option.value_kind == "int":
        return "i64"
    return "String"


def default_literal(option: OptionSpec) -> str | None:
    if option.default is None:
        return None
    if isinstance(option.default, bool):
        return str(option.default).lower()
    return str(option.default)


def render_clap(command: CommandSpec) -> str:
    lines = [
        f"// Generated option projection for weaveback {command.name}",
        f"// {command.summary}",
        "",
    ]
    for option in command.options:
        lines.append(f"/// {option.help_short}")
        if option.doc:
            lines.append(f"/// {option.doc}")
        arg_parts = [f'long = "{option.long}"']
        if option.short:
            arg_parts.append(f"short = '{option.short}'")
        default = default_literal(option)
        if default is not None:
            arg_parts.append(f'default_value = "{default}"')
        if option.repeatable:
            arg_parts.append('value_name = "VALUE"')
        lines.append(f"#[arg({', '.join(arg_parts)})]")
        lines.append(f"{option.id}: {rust_type_for(option)},")
        lines.append("")
    return "\n".join(lines).rstrip() + "\n"


def render_argparse(command: CommandSpec) -> str:
    lines = [
        f'"""Generated argparse projection for weaveback {command.name}."""',
        "",
        "from argparse import ArgumentParser",
        "from pathlib import Path",
        "",
        "",
        "def build_parser() -> ArgumentParser:",
        f'    parser = ArgumentParser(prog="weaveback {command.name}")',
        f'    parser.description = "{command.summary}"',
    ]
    for option in command.options:
        flags = [f'"--{option.long}"']
        if option.short:
            flags.append(f'"-{option.short}"')
        kwargs: list[str] = []
        if option.value_kind == "bool":
            kwargs.append('action="store_true"')
        elif option.value_kind == "path":
            kwargs.append("type=Path")
        elif option.value_kind == "int":
            kwargs.append("type=int")
        if option.default is not None:
            kwargs.append(f"default={option.default!r}")
        kwargs.append(f"help={option.help_short!r}")
        lines.append(f"    parser.add_argument({', '.join(flags + kwargs)})")
    lines.append("    return parser")
    lines.append("")
    return "\n".join(lines)


def render_adoc(command: CommandSpec) -> str:
    lines = [
        f"== `{command.name}` option projection",
        "",
        command.summary,
        "",
        command.rationale,
        "",
        '[cols="2,1,3,4,3",options="header"]',
        "|===",
        "| Flag | Type | Default | Description | Why",
        "",
    ]
    for option in command.options:
        default = default_literal(option) or ""
        option_type = option.value_kind
        flag = f"`--{option.long}`"
        if option.short:
            flag += f" / `-{option.short}`"
        lines.append(
            f"| {flag} | `{option_type}` | `{default}` | {option.help_short} | {option.rationale}"
        )
    lines.extend(["|===", ""])
    if command.examples:
        lines.append("=== Examples")
        lines.append("")
        lines.append("[source,bash]")
        lines.append("----")
        lines.extend(command.examples)
        lines.append("----")
        lines.append("")
    return "\n".join(lines)


def render_facts(command: CommandSpec) -> str:
    payload = {
        "command": {
            "name": command.name,
            "summary": command.summary,
            "rationale": command.rationale,
            "examples": list(command.examples),
        },
        "options": [
            {
                "id": option.id,
                "long": option.long,
                "short": option.short,
                "value_kind": option.value_kind,
                "default": option.default,
                "help_short": option.help_short,
                "rationale": option.rationale,
                "examples": list(option.examples),
                "doc": option.doc,
                "required": option.required,
                "repeatable": option.repeatable,
            }
            for option in command.options
        ],
    }
    return json.dumps(payload, indent=2, sort_keys=True) + "\n"


def render_all(spec_path: Path, out_dir: Path) -> None:
    command = load_spec(spec_path)
    out_dir.mkdir(parents=True, exist_ok=True)
    stem = command.name.replace("-", "_")
    (out_dir / f"{stem}_clap.rs.inc").write_text(render_clap(command), encoding="utf-8")
    (out_dir / f"{stem}_argparse.py").write_text(
        render_argparse(command), encoding="utf-8"
    )
    (out_dir / f"{stem}_options.adoc").write_text(render_adoc(command), encoding="utf-8")
    (out_dir / f"{stem}_facts.json").write_text(render_facts(command), encoding="utf-8")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--spec", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    render_all(args.spec, args.out)


if __name__ == "__main__":
    main()
