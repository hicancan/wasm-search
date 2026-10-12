const pagePaths = new Set([
    '/', '/search', '/community', '/materials', '/timetable', '/classrooms', '/exam',
]);

export const config = {
    matcher: [
        '/', '/index.html', '/search/:path*', '/community/:path*', '/materials/:path*',
        '/timetable/:path*', '/classrooms/:path*', '/exam/:path*',
    ],
};

export function middleware(context) {
    const response = context.next();
    const url = new URL(context.request.url);
    const pathname = url.pathname.replace(/\/index\.html$/, '/').replace(/\/$/, '') || '/';

    if (
        (context.request.method === 'GET' || context.request.method === 'HEAD')
        && url.search
        && pagePaths.has(pathname)
    ) {
        // EdgeOne merges headers on the next() response into the served page.
        response.headers.set('X-Robots-Tag', 'noindex, follow');
        response.headers.set('Cache-Control', 'private, no-store');
    }

    return response;
}
