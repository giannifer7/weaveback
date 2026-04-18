#!/usr/bin/env python3.14
import asyncio
import random
import sys
from dataclasses import dataclass
from typing import Final, NoReturn

type QuoteList = list[str]

RALPH_QUOTES: Final[QuoteList] = [
    "My cat's breath smells like cat food.",
    "I'm in danger.",
    "I'm a unit of measure!",
    "The pointy ones make me cry.",
    "I'm lernding!",
    "Me fail English? That's unpossible!",
    "It tastes like burning.",
    "I'm a computer!",
]

@dataclass(frozen=True)
class AIResponse:
    content: str
    success: bool

class AntigravityHeadlessWrapper:
    """A truly headless wrapper for the Antigravity (Gemini) CLI engine."""

    async def chat(self, prompt: str) -> AIResponse:
        """Calls the 'gemini' CLI in non-interactive mode."""

        # -p: non-interactive prompt mode
        # --output-format text: ensure we get clean string output
        cmd: list[str] = ["gemini", "-p", prompt, "--output-format", "text"]

        process = await asyncio.create_subprocess_exec(
            *cmd,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE
        )

        stdout, stderr = await process.communicate()

        if process.returncode != 0:
            error_msg = stderr.decode().strip()
            return AIResponse(f"CLI Error: {error_msg}", False)

        # Clean up output: gemini often prints API key warnings to stdout/stderr
        # We filter for the actual response content.
        content = stdout.decode().strip()
        lines = [line for line in content.splitlines()
                 if "Using GOOGLE_API_KEY" not in line and "MCP issues" not in line]

        return AIResponse("\n".join(lines).strip(), True)

async def start_ralph_loop() -> NoReturn:
    agent = AntigravityHeadlessWrapper()
    print("🚀 Starting the Headless Antigravity (Gemini) Ralph Wiggum Loop...\n")

    while True:
        quote = random.choice(RALPH_QUOTES)
        print(f"👦 Ralph says: \"{quote}\"")
        print("🤖 Antigravity is thinking...", end="\r")

        # Requesting a playful response from the Antigravity credits
        prompt = f"Give a funny, 1-sentence pseudo-philosophical response to this Ralph Wiggum quote: '{quote}'"
        response = await agent.chat(prompt)

        print(" " * 40, end="\r")  # Clear the 'thinking' line

        if response.success:
            print(f"💬 Response: {response.content}\n")
        else:
            print(f"⚠️ {response.content}\n")

        # Pause to let Ralph breathe
        await asyncio.sleep(5)

if __name__ == "__main__":
    try:
        asyncio.run(start_ralph_loop())
    except KeyboardInterrupt:
        print("\n👋 Ralph went home. Goodbye!")
        sys.exit(0)
