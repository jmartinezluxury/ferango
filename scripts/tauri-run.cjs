// Ensures ~/.cargo/bin is in PATH before calling the tauri CLI.
// Works on macOS, Linux, and Windows without extra dependencies.
const { join } = require('path');
const { homedir } = require('os');
const { spawnSync } = require('child_process');

const sep = process.platform === 'win32' ? ';' : ':';
const cargoBin = join(homedir(), '.cargo', 'bin');

const env = { ...process.env };
if (!(env.PATH || '').includes('.cargo')) {
  env.PATH = cargoBin + sep + (env.PATH || '');
}

const result = spawnSync('tauri', process.argv.slice(2), {
  stdio: 'inherit',
  shell: true,
  env,
});

process.exit(result.status ?? 1);
