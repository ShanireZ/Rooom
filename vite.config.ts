import { execFileSync } from 'node:child_process';
import { resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import babel from '@rolldown/plugin-babel';
import tailwind from '@tailwindcss/vite';
import react, { reactCompilerPreset } from '@vitejs/plugin-react';
import { defineConfig, type Plugin } from 'vite';

function compileOnEdit(): Plugin {
  return {
    name: 'rooom-compile-on-edit',
    apply: 'serve',
    configureServer(server) {
      const root = server.config.root;
      const sourceDirectory = resolve(root, 'src') + sep;
      const compiler = fileURLToPath(new URL('bin/tsc', import.meta.resolve('typescript/package.json')));
      server.watcher.on('all', (event, path) => {
        if (!['add', 'change', 'unlink'].includes(event) ||
            !resolve(path).startsWith(sourceDirectory) || !path.endsWith('.ts')) return;
        try {
          execFileSync(process.execPath, [compiler], { cwd: root, stdio: 'inherit' });
        } catch {
          server.config.logger.error('TypeScript compilation failed. The last successful build is still served.');
        }
      });
    },
  };
}

export default defineConfig({
  plugins: [compileOnEdit(), react(), babel({ presets: [reactCompilerPreset()] }), tailwind()],
  server: { host: '127.0.0.1', port: 1420, strictPort: true },
});
