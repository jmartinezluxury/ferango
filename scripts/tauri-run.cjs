// Ensures ~/.cargo/bin and node_modules/.bin are in PATH before calling tauri.
// Works on macOS, Linux, and Windows without extra dependencies.
const { join } = require('path');
const { homedir } = require('os');
const { spawnSync } = require('child_process');

const sep = process.platform === 'win32' ? ';' : ':';
const cargoBin = join(homedir(), '.cargo', 'bin');
const nmBin = join(__dirname, '..', 'node_modules', '.bin');

const env = { ...process.env };
env.PATH = [cargoBin, nmBin, env.PATH || ''].join(sep);

const result = spawnSync('tauri', process.argv.slice(2), {
  stdio: 'inherit',
  shell: true,
  env,
});

process.exit(result.status ?? 1);
