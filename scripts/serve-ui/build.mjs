import * as esbuild from 'esbuild';
import { writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const dir = dirname(fileURLToPath(import.meta.url));

const [jsResult, cssResult] = await Promise.all([
  esbuild.build({
    entryPoints: [`${dir}/src/index.ts`],
    bundle: true,
    minify: true,
    write: false,
    format: 'iife',
    target: ['chrome120', 'firefox121', 'safari17'],
    treeShaking: true,
  }),
  esbuild.build({
    entryPoints: [`${dir}/src/theme.css`],
    bundle: true,
    minify: true,
    write: false,
  }),
]);

const decode = (r) => new TextDecoder().decode(r.outputFiles[0].contents);
writeFileSync(join(dir, '../asciidoc-theme/wb-theme.js'),  decode(jsResult));
writeFileSync(join(dir, '../asciidoc-theme/wb-theme.css'), decode(cssResult));
console.log('serve-ui: built \u2192 wb-theme.js, wb-theme.css');
