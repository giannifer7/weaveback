#!/usr/bin/env python3.14
import asyncio
import random
import shlex
import sys
from dataclasses import dataclass
from typing import Final, NoReturn

# Modern type alias using the 'type' statement (Python 3.12+)
type PromptSource = list[str]

RALPH_QUOTES: Final[PromptSource] = [
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
class AntigravityResponse:
    content: str
    exit_code: int

class AntigravityWrapper:
    """A modern async wrapper for the Antigravity Agent CLI."""

    def __init__(self, mode: str = "agent") -> None:
        self.mode: str = mode

    async def chat(self, prompt: str) -> AntigravityResponse:
        """Calls 'antigravity chat' headlessly and returns the response."""
        
        # Build the command safely
        cmd: list[str] = ["antigravity", "chat", "-m", self.mode, prompt]
        
        process = await asyncio.create_subprocess_exec(
            *cmd,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE
        )

        stdout, stderr = await process.communicate()
        
        if process.returncode != 0:
            error_msg = stderr.decode().strip()
            return AntigravityResponse(f"Error: {error_msg}", process.returncode or 1)

        return AntigravityResponse(stdout.decode().strip(), 0)

async def start_ralph_loop() -> NoReturn:
    """Runs the Ralph Wiggum loop indefinitely."""
    
    agent = AntigravityWrapper(mode="agent")
    print("🚀 Starting the Ralph Wiggum Antigravity Loop...\n")

    while True:
        quote = random.choice(RALPH_QUOTES)
        print(f"👦 Ralph says: \"{quote}\"")
        print("🤖 Antigravity is thinking...", end="\r")

        # Ask the agent to respond to Ralph's wisdom
        prompt = f"How would an advanced AI respond to this statement from Ralph Wiggum: '{quote}'?"
        response = await agent.chat(prompt)

        print(" " * 40, end="\r")  # Clear 'thinking' line
        print(f"💬 Response: {response.content}\n")
        
        # Pause to let Ralph breathe
        await asyncio.sleep(4)

if __name__ == "__main__":
    try:
        asyncio.run(start_ralph_loop())
    except KeyboardInterrupt:
        print("\n👋 Ralph went home to eat paste. Goodbye!")
        sys.exit(0)
