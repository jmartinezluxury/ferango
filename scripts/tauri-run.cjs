// Ensures ~/.cargo/bin is in PATH and calls the tauri CLI by absolute path.
// Works on macOS, Linux, and Windows without extra dependencies.
const { join } = require('path');
const { homedir } = require('os');
const { spawnSync } = require('child_process');

const isWin = process.platform === 'win32';
const sep = isWin ? ';' : ':';
const cargoBin = join(homedir(), '.cargo', 'bin');
const nmBin = join(__dirname, '..', 'node_modules', '.bin');

const env = { ...process.env };
env.PATH = [cargoBin, nmBin, env.PATH || ''].join(sep);

// Use the absolute path to the binary so PATH resolution isn't needed.
// On Windows, .cmd files must be invoked with shell: true.
const ext = isWin ? '.cmd' : '';
const tauriBin = join(nmBin, `tauri${ext}`);

const result = spawnSync(tauriBin, process.argv.slice(2), {
  stdio: 'inherit',
  shell: isWin,
  env,
});

process.exit(result.status ?? 1);
