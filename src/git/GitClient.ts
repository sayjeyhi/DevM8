interface BunSubprocess {
  stdout: ReadableStream<Uint8Array>
  stderr: ReadableStream<Uint8Array>
  exited: Promise<number>
}

declare const Bun: {
  spawn(args: string[], opts: { stdout: 'pipe'; stderr: 'pipe'; cwd?: string }): BunSubprocess
}

export class GitClient {
  constructor(readonly repoPath: string) {}

  private async exec(args: string[]): Promise<{ stdout: string; stderr: string; exitCode: number }> {
    const proc = Bun.spawn(args, { stdout: 'pipe', stderr: 'pipe', cwd: this.repoPath })
    const [stdout, stderr, exitCode] = await Promise.all([
      new Response(proc.stdout).text(),
      new Response(proc.stderr).text(),
      proc.exited,
    ])
    return { stdout: stdout.trim(), stderr: stderr.trim(), exitCode }
  }

  async currentBranch(): Promise<string> {
    const { stdout, exitCode } = await this.exec(['git', 'rev-parse', '--abbrev-ref', 'HEAD'])
    if (exitCode !== 0) throw new Error('Failed to get current branch')
    return stdout
  }

  async isClean(): Promise<boolean> {
    const { stdout } = await this.exec(['git', 'status', '--porcelain'])
    return stdout.length === 0
  }

  async checkoutNewBranchFromMain(branchName: string, remote = 'origin', base = 'main'): Promise<void> {
    const fetch = await this.exec(['git', 'fetch', remote, base])
    if (fetch.exitCode !== 0) throw new Error(`git fetch failed: ${fetch.stderr}`)

    const checkout = await this.exec(['git', 'checkout', '-b', branchName, `${remote}/${base}`])
    if (checkout.exitCode !== 0) throw new Error(`git checkout failed: ${checkout.stderr}`)
  }

  async isGitRepo(): Promise<boolean> {
    const { exitCode } = await this.exec(['git', 'rev-parse', '--git-dir'])
    return exitCode === 0
  }
}
