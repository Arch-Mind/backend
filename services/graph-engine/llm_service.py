import json
import os
from dataclasses import dataclass
from typing import Any, Dict, List, Optional


@dataclass
class GeminiSettings:
    gemini_api_key: str
    model: str = "gemini-2.0-flash"

    @staticmethod
    def from_env() -> "GeminiSettings":
        key = os.getenv("GEMINI_API_KEY", "")
        if not key:
            raise ValueError("GEMINI_API_KEY environment variable is not set")
        return GeminiSettings(
            gemini_api_key=key,
            model=os.getenv("GEMINI_MODEL", "gemini-2.0-flash"),
        )


# Backward-compatible alias kept so existing imports in main.py continue to work
LLMSettings = GeminiSettings


def call_gemini(prompt: str, settings: GeminiSettings) -> str:
    if not settings.gemini_api_key:
        raise ValueError("GEMINI_API_KEY is required")

    try:
        from google import genai

        os.environ["GEMINI_API_KEY"] = settings.gemini_api_key
        client = genai.Client()

        response = client.models.generate_content(
            model=settings.model,
            contents=prompt,
        )
        return response.text.strip()
    except Exception as e:
        raise ValueError(f"Gemini API error: {str(e)}")


# Backward-compatible alias
def call_llm(prompt: str, settings: GeminiSettings) -> str:
    return call_gemini(prompt, settings)


def build_pattern_prompt(summary: Dict[str, Any]) -> str:
    language_dist = summary.get("language_dist", {})
    top_files = summary.get("top_files_by_degree", [])
    circular_count = summary.get("circular_dep_count", 0)
    edge_density = summary.get("edge_density", 0.0)
    file_count = summary.get("file_count", 0)
    boundaries = summary.get("boundaries", [])

    return (
        "You are a senior software architect analysing a codebase dependency graph.\n"
        "\nRepository stats:\n"
        f"- Total files: {file_count}\n"
        f"- Language distribution: {json.dumps(language_dist)}\n"
        f"- Modules/boundaries: {len(boundaries)}\n"
        f"- Circular dependency chains: {circular_count}\n"
        f"- Edge density (avg deps per file): {edge_density:.2f}\n"
        f"- Most-connected files (top 10 by out-degree): {json.dumps(top_files[:10])}\n"
        "\nBased on these metrics, classify the overall architecture and identify patterns and anti-patterns.\n"
        "\nReturn ONLY valid JSON (no markdown fences) with this exact schema:\n"
        "{\n"
        '  "pattern_type": "<one of: monolithic | microservices | layered | hexagonal | event-driven | mixed>",\n'
        '  "confidence": <float 0.0-1.0>,\n'
        '  "summary": "<2-3 sentence plain-English overview of the architecture>",\n'
        '  "patterns_found": ["<pattern name>"],\n'
        '  "antipatterns": [{"name": "<name>", "severity": "<critical|high|medium|low>", "description": "<one sentence>"}],\n'
        '  "recommendations": ["<actionable recommendation>"]\n'
        "}"
    )


def build_module_summary_prompt(module_name: str, files: List[str], dependencies: List[Dict]) -> str:
    return (
        "You are a software architect.\n"
        "Summarise this module's purpose and role in the architecture in 50-100 words.\n"
        f"\nModule: {module_name}\n"
        f"Files:\n{json.dumps(files, indent=2)}\n"
        f"Dependencies (sample):\n{json.dumps(dependencies[:20], indent=2)}\n"
        "\nReturn ONLY valid JSON (no markdown fences):\n"
        '{"summary": "<text>", "role": "<presentation|business-logic|data-access|infrastructure|utility|api|config|test>", "coupling_concern": "<high|medium|low>"}'
    )


def build_file_summary_prompt(
    file_path: str,
    language: str,
    functions: List[str],
    classes: List[str],
    imports_list: List[str],
    dependents: List[str],
) -> str:
    return (
        "You are a software architect.\n"
        "Summarise this file's role in the codebase in 40-80 words.\n"
        f"\nFile: {file_path}\n"
        f"Language: {language or 'unknown'}\n"
        f"Classes: {json.dumps(classes[:10])}\n"
        f"Functions: {json.dumps(functions[:10])}\n"
        f"Imports: {json.dumps(imports_list[:15])}\n"
        f"Depended on by: {json.dumps(dependents[:10])}\n"
        "\nReturn ONLY valid JSON (no markdown fences):\n"
        '{"summary": "<text>", "role": "<presentation|business-logic|data-access|infrastructure|utility|api|config|test>", "coupling_concern": "<high|medium|low>"}'
    )


def parse_json_response(text: str) -> Dict[str, Any]:
    stripped = text.strip()
    if stripped.startswith("```"):
        lines = stripped.split("\n")
        inner = lines[1:-1] if lines[-1].strip() == "```" else lines[1:]
        stripped = "\n".join(inner).strip()
    try:
        return json.loads(stripped)
    except json.JSONDecodeError:
        return {"summary": text}
