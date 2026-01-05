"use client";

import { useState } from "react";

export default function Home() {
  const [repoUrl, setRepoUrl] = useState("");
  const [loading, setLoading] = useState(false);
  const [jobId, setJobId] = useState<string | null>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setLoading(true);

    try {
      const response = await fetch("http://localhost:8080/api/v1/analyze", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          repo_url: repoUrl,
          branch: "main",
        }),
      });

      const data = await response.json();
      setJobId(data.job_id);
    } catch (error) {
      console.error("Error submitting repository:", error);
      alert("Failed to submit repository for analysis");
    } finally {
      setLoading(false);
    }
  };

  return (
    <main className="min-h-screen bg-gradient-to-br from-slate-900 via-purple-900 to-slate-900">
      <div className="container mx-auto px-4 py-16">
        {/* Header */}
        <div className="text-center mb-16">
          <h1 className="text-6xl font-bold text-white mb-4">
            Arch<span className="text-purple-400">Mind</span>
          </h1>
          <p className="text-xl text-gray-300">
            Real-Time Codebase Intelligence & Architecture Reconstruction
          </p>
        </div>

        {/* Main Card */}
        <div className="max-w-4xl mx-auto bg-white/10 backdrop-blur-lg rounded-2xl shadow-2xl p-8 border border-white/20">
          <div className="mb-8">
            <h2 className="text-3xl font-semibold text-white mb-2">
              Analyze Your Repository
            </h2>
            <p className="text-gray-300">
              Enter a repository URL to start analyzing dependencies and architecture
            </p>
          </div>

          {/* Form */}
          <form onSubmit={handleSubmit} className="space-y-6">
            <div>
              <label
                htmlFor="repo-url"
                className="block text-sm font-medium text-gray-200 mb-2"
              >
                Repository URL
              </label>
              <input
                id="repo-url"
                type="url"
                value={repoUrl}
                onChange={(e) => setRepoUrl(e.target.value)}
                placeholder="https://github.com/username/repository"
                className="w-full px-4 py-3 bg-white/5 border border-white/20 rounded-lg text-white placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-purple-500 focus:border-transparent"
                required
              />
            </div>

            <button
              type="submit"
              disabled={loading}
              className="w-full bg-gradient-to-r from-purple-600 to-blue-600 hover:from-purple-700 hover:to-blue-700 text-white font-semibold py-3 px-6 rounded-lg transition-all duration-200 disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {loading ? "Submitting..." : "Analyze Repository"}
            </button>
          </form>

          {/* Job Status */}
          {jobId && (
            <div className="mt-8 p-4 bg-green-500/20 border border-green-500/50 rounded-lg">
              <p className="text-green-200">
                <span className="font-semibold">Job Created:</span> {jobId}
              </p>
              <p className="text-sm text-green-300 mt-2">
                Your repository is being analyzed. Check the status in the dashboard.
              </p>
            </div>
          )}
        </div>

        {/* Features Grid */}
        <div className="max-w-6xl mx-auto mt-16 grid md:grid-cols-3 gap-8">
          <div className="bg-white/5 backdrop-blur-sm rounded-xl p-6 border border-white/10">
            <div className="text-4xl mb-4">🔍</div>
            <h3 className="text-xl font-semibold text-white mb-2">
              Deep Analysis
            </h3>
            <p className="text-gray-400">
              Parse multiple languages with tree-sitter for comprehensive insights
            </p>
          </div>

          <div className="bg-white/5 backdrop-blur-sm rounded-xl p-6 border border-white/10">
            <div className="text-4xl mb-4">📊</div>
            <h3 className="text-xl font-semibold text-white mb-2">
              Graph Visualization
            </h3>
            <p className="text-gray-400">
              Interactive 3D dependency graphs powered by WebGL
            </p>
          </div>

          <div className="bg-white/5 backdrop-blur-sm rounded-xl p-6 border border-white/10">
            <div className="text-4xl mb-4">⚡</div>
            <h3 className="text-xl font-semibold text-white mb-2">
              Real-Time Updates
            </h3>
            <p className="text-gray-400">
              Watch your architecture evolve with live analysis results
            </p>
          </div>
        </div>

        {/* Stats */}
        <div className="max-w-4xl mx-auto mt-16 text-center">
          <div className="grid grid-cols-3 gap-8">
            <div>
              <div className="text-4xl font-bold text-purple-400">5+</div>
              <div className="text-gray-400 mt-2">Languages Supported</div>
            </div>
            <div>
              <div className="text-4xl font-bold text-blue-400">∞</div>
              <div className="text-gray-400 mt-2">Repositories</div>
            </div>
            <div>
              <div className="text-4xl font-bold text-green-400">100%</div>
              <div className="text-gray-400 mt-2">Open Source</div>
            </div>
          </div>
        </div>
      </div>
    </main>
            >
              Learning
            </a>{" "}
            center.
          </p>
        </div>
        <div className="flex flex-col gap-4 text-base font-medium sm:flex-row">
          <a
            className="flex h-12 w-full items-center justify-center gap-2 rounded-full bg-foreground px-5 text-background transition-colors hover:bg-[#383838] dark:hover:bg-[#ccc] md:w-[158px]"
            href="https://vercel.com/new?utm_source=create-next-app&utm_medium=appdir-template-tw&utm_campaign=create-next-app"
            target="_blank"
            rel="noopener noreferrer"
          >
            <Image
              className="dark:invert"
              src="/vercel.svg"
              alt="Vercel logomark"
              width={16}
              height={16}
            />
            Deploy Now
          </a>
          <a
            className="flex h-12 w-full items-center justify-center rounded-full border border-solid border-black/[.08] px-5 transition-colors hover:border-transparent hover:bg-black/[.04] dark:border-white/[.145] dark:hover:bg-[#1a1a1a] md:w-[158px]"
            href="https://nextjs.org/docs?utm_source=create-next-app&utm_medium=appdir-template-tw&utm_campaign=create-next-app"
            target="_blank"
            rel="noopener noreferrer"
          >
            Documentation
          </a>
        </div>
      </main>
    </div>
  );
}
