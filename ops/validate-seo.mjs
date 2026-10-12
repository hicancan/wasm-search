import { readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

const argumentIndex = process.argv.indexOf('--dist');
const distArgument = argumentIndex >= 0 ? process.argv[argumentIndex + 1] : process.env.NJUPT_SEARCH_WEB_OUT_DIR;
if (!distArgument) throw new Error('SEO validation requires --dist or NJUPT_SEARCH_WEB_OUT_DIR');
const dist = resolve(distArgument);

const read = relativePath => readFileSync(join(dist, relativePath), 'utf8');
const failures = [];
const assert = (condition, message) => {
    if (!condition) failures.push(message);
};
const occurrences = (value, pattern) => value.match(pattern)?.length ?? 0;

const pages = [
    {
        file: 'index.html',
        title: 'njupt-search｜南邮网站与教务查询',
        description: '搜索南邮校方信息、校园经验和课程资料，查询班级课表、考试安排与教室空间。',
        canonical: 'https://njupt.hicancan.top/',
        h1: 'njupt-search',
        structured: true,
    },
    {
        file: 'search/index.html',
        title: '南邮网站搜索｜njupt-search',
        description: '搜索南京邮电大学各网站的通知、附件和办事信息。',
        canonical: 'https://njupt.hicancan.top/search',
        h1: '南邮网站搜索',
    },
    {
        file: 'exam/index.html',
        title: '南邮考试安排查询｜njupt-search',
        description: '输入班级号，查询南京邮电大学考试时间、地点和考场，并可导出日历。',
        canonical: 'https://njupt.hicancan.top/exam',
        h1: '查询考试安排',
    },
    {
        file: 'community/index.html',
        title: '南邮社区搜索｜njupt-search',
        description: '搜索南邮生存手册中的校园生活、学习经验和办事指南。',
        canonical: 'https://njupt.hicancan.top/community',
        h1: '南邮社区搜索',
    },
    {
        file: 'materials/index.html',
        title: '南邮资料搜索｜njupt-search',
        description: '搜索南京邮电大学课程资料、历年试卷、课件和复习笔记。',
        canonical: 'https://njupt.hicancan.top/materials',
        h1: '南邮资料搜索',
    },
    {
        file: 'timetable/index.html',
        title: '南邮班级课表查询｜njupt-search',
        description: '输入班级号，按周查看南京邮电大学课程时间、教师和教室，并可导出日历。',
        canonical: 'https://njupt.hicancan.top/timetable',
        h1: '查询班级课表',
    },
    {
        file: 'classrooms/index.html',
        title: '南邮教室空间浏览｜njupt-search',
        description: '按校区、楼栋和楼层浏览南京邮电大学教室，并查看所选时段的课程与考试占用。',
        canonical: 'https://njupt.hicancan.top/classrooms',
        h1: '教室',
    },
];

for (const page of pages) {
    const html = read(page.file);
    assert(html.includes(`<title>${page.title}</title>`), `${page.file}: title mismatch`);
    assert(html.includes(`name="description" content="${page.description}"`), `${page.file}: description mismatch`);
    assert(html.includes('name="robots" content="index, follow"'), `${page.file}: index policy mismatch`);
    assert(html.includes(`rel="canonical" href="${page.canonical}"`), `${page.file}: canonical mismatch`);
    assert(html.includes('property="og:site_name" content="njupt-search"'), `${page.file}: missing og:site_name`);
    assert(html.includes('property="og:image:alt" content="njupt-search 标志"'), `${page.file}: missing image alt`);
    assert(html.includes(`>${page.h1}</h1>`), `${page.file}: expected H1 content is missing`);
    assert(occurrences(html, /<h1\b/g) === 1, `${page.file}: expected exactly one H1`);
    assert(html.includes('data-seo-query-policy'), `${page.file}: missing query noindex guard`);
    assert(!html.includes('南邮通知、考试安排和考试教室，都可以直接查。'), `${page.file}: deleted home copy returned`);
    assert(!html.includes('meta name="keywords"'), `${page.file}: obsolete keywords metadata returned`);
    if (page.structured) {
        const match = html.match(/<script type="application\/ld\+json">([\s\S]*?)<\/script>/);
        assert(Boolean(match), `${page.file}: missing WebSite JSON-LD`);
        if (match) {
            const data = JSON.parse(match[1]);
            assert(data['@type'] === 'WebSite', `${page.file}: JSON-LD is not WebSite`);
            assert(!html.includes('"@type":"Organization"'), `${page.file}: false Organization structured data`);
        }
    } else {
        assert(!html.includes('<script type="application/ld+json">'), `${page.file}: WebSite JSON-LD must be homepage-only`);
    }
}

const home = read('index.html');
assert(home.includes('<meta name="msvalidate.01" content="E44C2F6A09F0A678DED98C8374E549A1" />'), 'index.html: missing Bing ownership verification');
assert(home.includes('<meta name="baidu-site-verification" content="codeva-3WJsMJj16W" />'), 'index.html: missing Baidu ownership verification');
for (const href of ['/search', '/community', '/materials', '/exam', '/timetable', '/classrooms']) {
    assert(home.includes(`href="${href}"`), `index.html: missing crawlable ${href} link`);
}

const notFound = read('404.html');
assert(notFound.includes('name="robots" content="noindex, follow"'), '404.html: missing noindex');
assert(!notFound.includes('<script type="module"'), '404.html: must not start the application');
assert(occurrences(notFound, /<h1\b/g) === 1, '404.html: expected exactly one H1');

const robots = read('robots.txt').trim();
assert(robots === 'User-agent: *\nAllow: /\n\nSitemap: https://njupt.hicancan.top/sitemap.xml', 'robots.txt: content mismatch');

const sitemap = read('sitemap.xml');
const locations = [...sitemap.matchAll(/<loc>([^<]+)<\/loc>/g)].map(match => match[1]);
assert(JSON.stringify(locations) === JSON.stringify([
    'https://njupt.hicancan.top/',
    'https://njupt.hicancan.top/search',
    'https://njupt.hicancan.top/exam',
    'https://njupt.hicancan.top/timetable',
    'https://njupt.hicancan.top/classrooms',
    'https://njupt.hicancan.top/community',
    'https://njupt.hicancan.top/materials',
]), 'sitemap.xml: canonical URL set mismatch');
assert(locations.every(location => !location.includes('#') && !location.includes('?')), 'sitemap.xml: hash or query URL found');
assert(sitemap.startsWith('<?xml version="1.0" encoding="UTF-8"?>'), 'sitemap.xml: invalid XML declaration');

const edgeone = JSON.parse(read('edgeone.json'));
const redirects = edgeone.redirects ?? [];
const rewrites = edgeone.rewrites ?? [];
assert(JSON.stringify(redirects) === JSON.stringify([
    { source: '/index.html', destination: '/', statusCode: 301 },
    { source: '/exam/', destination: '/exam', statusCode: 301 },
    { source: '/search/', destination: '/search', statusCode: 301 },
    { source: '/timetable/', destination: '/timetable', statusCode: 301 },
    { source: '/classrooms/', destination: '/classrooms', statusCode: 301 },
    { source: '/community/', destination: '/community', statusCode: 301 },
    { source: '/materials/', destination: '/materials', statusCode: 301 },
]), 'edgeone.json: canonical redirects mismatch');
assert(JSON.stringify(rewrites) === JSON.stringify([
    { source: '/exam', destination: '/exam/index.html' },
    { source: '/search', destination: '/search/index.html' },
    { source: '/timetable', destination: '/timetable/index.html' },
    { source: '/classrooms', destination: '/classrooms/index.html' },
    { source: '/community', destination: '/community/index.html' },
    { source: '/materials', destination: '/materials/index.html' },
]), 'edgeone.json: clean route rewrites mismatch');
const redirectSources = new Set(redirects.map(redirect => redirect.source));
for (const rewrite of rewrites) {
    assert(!redirectSources.has(rewrite.destination), `edgeone.json: rewrite destination ${rewrite.destination} is redirected and cannot be served`);
    const artifactPath = rewrite.destination.replace(/^\//, '');
    try {
        read(artifactPath);
    } catch {
        assert(false, `edgeone.json: rewrite destination ${rewrite.destination} is missing from dist`);
    }
}
assert(!rewrites.some(rewrite => rewrite.source.includes('*')), 'edgeone.json: catch-all page fallback found');
assert(!(edgeone.headers ?? []).some(rule => rule.source === '/search'), 'edgeone.json: stable search landing must not be noindex');
const cacheControl = source => (edgeone.headers ?? [])
    .find(rule => rule.source === source)?.headers
    .find(header => header.key.toLowerCase() === 'cache-control')?.value;
assert(cacheControl('/*') === undefined, 'edgeone.json: global cache override can make error responses sticky');
assert(cacheControl('/assets/*') === undefined, 'edgeone.json: asset wildcard cache can make transient 404 responses immutable');
for (const source of ['/generated/search/*', '/generated/exam/*', '/generated/rooms/*', '/generated/timetable/*', '/generated/classrooms/*', '/generated/space/*']) {
    const headers = (edgeone.headers ?? []).find(rule => rule.source === source)?.headers ?? [];
    assert(headers.some(header => header.key === 'X-Robots-Tag' && header.value === 'noindex, nofollow'), `edgeone.json: ${source} robots header is missing`);
}
const headerRuleIndex = source => (edgeone.headers ?? []).findIndex(rule => rule.source === source);
for (const domain of ['search', 'rooms', 'timetable', 'classrooms', 'space']) {
    assert(
        headerRuleIndex(`/generated/${domain}/manifest.json`) > headerRuleIndex(`/generated/${domain}/*`),
        `edgeone.json: stable ${domain} manifest rule must follow its immutable wildcard`,
    );
}
assert(headerRuleIndex('/generated/exam/manifest.json') > headerRuleIndex('/generated/exam/*'), 'edgeone.json: exam manifest rule order is unsafe');
assert(headerRuleIndex('/generated/exam/history/manifest.json') > headerRuleIndex('/generated/exam/*'), 'edgeone.json: exam history manifest rule order is unsafe');

for (const file of ['index.html', 'exam/index.html', 'search/index.html', 'community/index.html', 'materials/index.html', 'timetable/index.html', 'classrooms/index.html']) {
    const html = read(file);
    assert(!html.includes('#/'), `${file}: hash route found in production HTML`);
    assert(!/<(?:div|section|p|h[1-6])[^>]*class="[^"]*(?:sr-only|hidden)[^"]*"[^>]*>[^<]*(?:南邮校园信息|考试安排|考试教室)/.test(html), `${file}: hidden SEO copy found`);
}

if (failures.length) {
    throw new Error(`SEO validation failed:\n- ${failures.join('\n- ')}`);
}
console.log(`SEO validation passed for ${dist}`);
