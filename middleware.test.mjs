import assert from 'node:assert/strict';
import test from 'node:test';
import { middleware } from './middleware.js';

const serve = (path, method = 'GET') => {
    const response = new Response(null, {
        headers: { 'x-middleware-next': '1', 'x-existing-header': 'preserved' },
    });
    let nextCalls = 0;
    const result = middleware({
        request: new Request(`https://njupt.hicancan.top${path}`, { method }),
        next: (...args) => {
            nextCalls += 1;
            assert.equal(args.length, 0, 'response headers must not become request overrides');
            return response;
        },
    });
    assert.equal(result, response);
    assert.equal(nextCalls, 1);
    assert.equal(result.headers.get('x-middleware-next'), '1');
    assert.equal(result.headers.get('x-existing-header'), 'preserved');
    return result;
};

const publicPaths = ['/', '/search', '/community', '/materials', '/timetable', '/classrooms', '/exam'];

test('parameterized product pages send noindex to crawlers without JavaScript', () => {
    for (const path of publicPaths) {
        const response = serve(`${path}?q=test&class=B260101`);
        assert.equal(response.headers.get('X-Robots-Tag'), 'noindex, follow', path);
        assert.equal(response.headers.get('Cache-Control'), 'private, no-store', path);
    }
});

test('parameterized HEAD requests have the same crawl policy', () => {
    assert.equal(serve('/search?q=test', 'HEAD').headers.get('X-Robots-Tag'), 'noindex, follow');
});

test('canonical pages remain indexable and retain their cache policy', () => {
    for (const path of publicPaths) {
        const response = serve(path);
        assert.equal(response.headers.get('X-Robots-Tag'), null, path);
        assert.equal(response.headers.get('Cache-Control'), null, path);
    }
});

test('static HTML and trailing-slash aliases obey the same query policy', () => {
    for (const path of ['/index.html', '/search/', '/search/index.html', '/exam/', '/exam/index.html']) {
        assert.equal(serve(`${path}?q=test`).headers.get('X-Robots-Tag'), 'noindex, follow', path);
        assert.equal(serve(path).headers.get('X-Robots-Tag'), null, path);
    }
});

test('sitemaps, robots, verification files, assets and generated data are unaffected', () => {
    for (const path of [
        '/sitemap.xml', '/robots.txt', '/BingSiteAuth.xml', '/google-verification.html',
        '/assets/app.js', '/generated/search/manifest.json', '/search/worker.wasm',
        '/not-a-product-page',
    ]) {
        const response = serve(`${path}?v=123`);
        assert.equal(response.headers.get('X-Robots-Tag'), null, path);
        assert.equal(response.headers.get('Cache-Control'), null, path);
    }
});

test('empty queries and non-page methods do not acquire the indexing header', () => {
    assert.equal(serve('/search?').headers.get('X-Robots-Tag'), null);
    assert.equal(serve('/search?q=test', 'POST').headers.get('X-Robots-Tag'), null);
});
