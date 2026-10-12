import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import { parseEdgeOneDeployResult } from './parse-edgeone-deploy-result.mjs';
import { createEdgeOneProjectConfig } from './prepare-edgeone-project.mjs';

const success = {
  status: 'success',
  url: 'https://njupt-search.example.edgeone.cool?eo_token=token&eo_time=123',
  type: 'direct-upload',
  projectId: 'pages-example',
  deploymentId: 'deployment-example',
  consoleUrl: 'https://console.cloud.tencent.com/edgeone/pages/project/pages-example/deployment/deployment-example',
};

test('parses the final JSON line and preserves the complete deployment URL', () => {
  const output = `[cli] uploading\r\n${JSON.stringify(success)}\r\n`;
  const result = parseEdgeOneDeployResult(output);

  assert.deepEqual(result, success);
  assert.equal(result.url, success.url);
});

test('rejects an EdgeOne error result', () => {
  assert.throws(
    () => parseEdgeOneDeployResult('{"status":"error","error":"permission denied"}\n'),
    /permission denied/u,
  );
});

test('rejects a success result with missing identity fields', () => {
  const incomplete = { ...success };
  delete incomplete.deploymentId;

  assert.throws(
    () => parseEdgeOneDeployResult(JSON.stringify(incomplete)),
    /missing deploymentId/u,
  );
});

test('rejects text output instead of structured output', () => {
  assert.throws(
    () => parseEdgeOneDeployResult('EDGEONE_DEPLOY_URL=https://example.com\n'),
    /not valid JSON/u,
  );
});

test('production deploy uses the Makers namespace and structured output only', () => {
  const deployScript = readFileSync(
    new URL('./deploy-edgeone-with-retry.sh', import.meta.url),
    'utf8',
  );

  assert.match(deployScript, /edgeone@latest makers deploy/u);
  assert.match(deployScript, /--json/u);
  assert.match(deployScript, /PAGES_SOURCE=skills/u);
  assert.doesNotMatch(deployScript, /edgeone@latest pages deploy/u);
});

test('deployment retry recognizes the Axios region timeout seen in production', () => {
  const deployScript = readFileSync(
    new URL('./deploy-edgeone-with-retry.sh', import.meta.url),
    'utf8',
  );

  assert.ok(deployScript.includes('AxiosError: timeout'));
  assert.ok(deployScript.includes('timeout of [0-9]+ms exceeded'));
  assert.ok(deployScript.includes('ECONNABORTED'));
});

test('project deployment compiles middleware and retains the verified artifact configuration', () => {
  const artifactConfig = JSON.parse(readFileSync(new URL('../../apps/web/public/edgeone.json', import.meta.url), 'utf8'));
  const projectConfig = createEdgeOneProjectConfig(artifactConfig);

  assert.deepEqual(projectConfig.redirects, artifactConfig.redirects);
  assert.deepEqual(projectConfig.rewrites, artifactConfig.rewrites);
  assert.deepEqual(projectConfig.headers, artifactConfig.headers);
  assert.equal(projectConfig.outputDirectory, 'dist');
  assert.equal(projectConfig.buildCommand, 'node ops/validate-seo.mjs --dist dist');
  assert.equal(projectConfig.installCommand, 'node --version');
  assert.equal(artifactConfig.buildCommand, undefined);

  const preparation = readFileSync(new URL('./prepare-edgeone-project.mjs', import.meta.url), 'utf8');
  assert.match(preparation, /readFileSync\('dist\/edgeone\.json'/u);
  const workflow = readFileSync(new URL('../workflows/deploy-edgeone.yml', import.meta.url), 'utf8');
  assert.match(workflow, /node \.github\/scripts\/prepare-edgeone-project\.mjs/u);
  assert.match(workflow, /deploy-edgeone-with-retry\.sh "" "\$project" production "\$area"/u);
  assert.doesNotMatch(workflow, /deploy-edgeone-with-retry\.sh \.\/dist/u);

  const deployScript = readFileSync(new URL('./deploy-edgeone-with-retry.sh', import.meta.url), 'utf8');
  assert.match(deployScript, /if \[ -n "\$dist_dir" \]; then\s+deploy_command\+=\("\$dist_dir"\)/u);
  assert.match(deployScript, /PAGES_SOURCE=skills "\$\{deploy_command\[@\]\}"/u);
});

test('artifact pointer workflows use the GitHub variable retry wrapper', () => {
  const retryScript = readFileSync(
    new URL('./gh-variable-set-with-retry.sh', import.meta.url),
    'utf8',
  );
  assert.match(retryScript, /gh variable set/u);
  assert.match(retryScript, /HTTP \(429\|5\[0-9\]\{2\}\)/u);

  const workflows = [
    '../workflows/build-corpus.yml',
    '../workflows/update-exam-data.yml',
    '../workflows/publish-teaching-source.yml',
  ].map(relative => readFileSync(new URL(relative, import.meta.url), 'utf8'));

  for (const workflow of workflows) {
    assert.match(workflow, /gh-variable-set-with-retry\.sh/u);
    assert.doesNotMatch(workflow, /\bgh variable set\b/u);
  }
});
