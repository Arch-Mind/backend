import json
import os
from dataclasses import dataclass
from typing import Any, Dict, Optional

import httpx


@dataclass
class LLMSettings:
    provider: str
    model: str
    openai_api_key: Optional[str]
    anthropic_api_key: Optional[str]
    gemini_api_key: Optional[str]
    ollama_url: str
    aws_region: str
    bedrock_model_id: str

    @staticmethod
    def from_env() -> "LLMSettings":
        return LLMSettings(
            provider=os.getenv("LLM_PROVIDER", "openai"),
            model=os.getenv("LLM_MODEL", "gpt-4"),
            openai_api_key=os.getenv("OPENAI_API_KEY"),
            anthropic_api_key=os.getenv("ANTHROPIC_API_KEY"),
            gemini_api_key=os.getenv("GEMINI_API_KEY"),
            ollama_url=os.getenv("OLLAMA_URL", "http://localhost:11434"),
            aws_region=os.getenv("AWS_REGION", "us-east-1"),
            bedrock_model_id=os.getenv("BEDROCK_MODEL_ID", "anthropic.claude-3-haiku-20240307-v1:0"),
        )


def call_llm(prompt: str, settings: LLMSettings) -> str:
    provider = settings.provider.lower()

    if provider == "openai":
        return call_openai(prompt, settings)
    if provider == "anthropic":
        return call_anthropic(prompt, settings)
    if provider == "gemini":
        return call_gemini(prompt, settings)
    if provider == "ollama":
        return call_ollama(prompt, settings)
    if provider == "bedrock":
        return call_bedrock(prompt, settings)

    raise ValueError(f"Unsupported LLM provider: {settings.provider}")


def call_openai(prompt: str, settings: LLMSettings) -> str:
    if not settings.openai_api_key:
        raise ValueError("OPENAI_API_KEY is required for OpenAI provider")

    headers = {
        "Authorization": f"Bearer {settings.openai_api_key}",
        "Content-Type": "application/json",
    }
    payload = {
        "model": settings.model,
        "messages": [{"role": "user", "content": prompt}],
        "temperature": 0.2,
        "max_tokens": 900,
    }

    with httpx.Client(timeout=60) as client:
        resp = client.post("https://api.openai.com/v1/chat/completions", json=payload, headers=headers)
        resp.raise_for_status()
        data = resp.json()
        return data["choices"][0]["message"]["content"].strip()


def call_anthropic(prompt: str, settings: LLMSettings) -> str:
    if not settings.anthropic_api_key:
        raise ValueError("ANTHROPIC_API_KEY is required for Anthropic provider")

    headers = {
        "x-api-key": settings.anthropic_api_key,
        "anthropic-version": "2023-06-01",
        "content-type": "application/json",
    }
    payload = {
        "model": settings.model,
        "max_tokens": 900,
        "temperature": 0.2,
        "messages": [{"role": "user", "content": prompt}],
    }

    with httpx.Client(timeout=60) as client:
        resp = client.post("https://api.anthropic.com/v1/messages", json=payload, headers=headers)
        resp.raise_for_status()
        data = resp.json()
        content = data.get("content", [])
        if content:
            return content[0].get("text", "").strip()
        return ""


def call_ollama(prompt: str, settings: LLMSettings) -> str:
    payload = {
        "model": settings.model,
        "prompt": prompt,
        "stream": False,
        "options": {"temperature": 0.2},
    }

    with httpx.Client(timeout=60) as client:
        resp = client.post(f"{settings.ollama_url.rstrip('/')}/api/generate", json=payload)
        resp.raise_for_status()
        data = resp.json()
        return data.get("response", "").strip()


def call_bedrock(prompt: str, settings: LLMSettings) -> str:
    import boto3

    client = boto3.client("bedrock-runtime", region_name=settings.aws_region)
    payload = {
        "anthropic_version": "bedrock-2023-05-31",
        "max_tokens": 900,
        "temperature": 0.2,
        "messages": [{"role": "user", "content": prompt}],
    }

    response = client.invoke_model(
        modelId=settings.bedrock_model_id,
        body=json.dumps(payload),
        accept="application/json",
        contentType="application/json",
    )

    body = response.get("body")
    if hasattr(body, "read"):
        body = body.read()

    data = json.loads(body)
    content = data.get("content", [])
    if content:
        return content[0].get("text", "").strip()

    return data.get("completion", "").strip()


def call_gemini(prompt: str, settings: LLMSettings) -> str:
    if not settings.gemini_api_key:
        raise ValueError("GEMINI_API_KEY is required for Gemini provider")

    model = settings.model or "gemini-1.5-flash"
    url = f"https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent?key={settings.gemini_api_key}"
    
    payload = {
        "contents": [
            {
                "parts": [
                    {"text": prompt}
                ]
            }
        ],
        "generationConfig": {
            "temperature": 0.2,
            "maxOutputTokens": 900,
        }
    }

    with httpx.Client(timeout=60) as client:
        resp = client.post(url, json=payload)
        resp.raise_for_status()
        data = resp.json()
        
        candidates = data.get("candidates", [])
        if candidates:
            content = candidates[0].get("content", {})
            parts = content.get("parts", [])
            if parts:
                return parts[0].get("text", "").strip()
        
        return ""


def build_pattern_prompt(summary: Dict[str, Any]) -> str:
    return "\n".join(
        [
            "You are an architecture analyst.",
            "Given the graph summary, classify the architecture pattern as one of:",
            "monolithic, microservices, layered, hexagonal, event-driven, or mixed.",
            "Return JSON with fields: pattern_type, confidence (0-1), summary.",
            "",
            "Graph summary:",
            json.dumps(summary, indent=2),
        ]
    )


def build_module_summary_prompt(module_name: str, files: list, dependencies: list) -> str:
    return "\n".join(
        [
            "You are an architecture analyst.",
            "Summarize this module's purpose and role in the architecture in 50-100 words.",
            "Return JSON with fields: summary.",
            "",
            f"Module: {module_name}",
            "Files:",
            json.dumps(files, indent=2),
            "Dependencies:",
            json.dumps(dependencies, indent=2),
        ]
    )


def parse_json_response(text: str) -> Dict[str, Any]:
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        return {"summary": text}
