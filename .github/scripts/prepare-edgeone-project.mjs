import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

export const createEdgeOneProjectConfig = (artifactConfig) => ({
    ...artifactConfig,
    installCommand: 'node --version',
    buildCommand: 'node ops/validate-seo.mjs --dist dist',
    outputDirectory: 'dist',
});

if (process.argv[1] === fileURLToPath(import.meta.url)) {
    const artifactConfig = JSON.parse(readFileSync('dist/edgeone.json', 'utf8'));
    writeFileSync('edgeone.json', `${JSON.stringify(createEdgeOneProjectConfig(artifactConfig), null, 2)}\n`);
}
