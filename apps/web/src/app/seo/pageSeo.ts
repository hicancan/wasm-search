import type { AppRoute } from '@/app/routing/useUrlState';

export const SITE_ORIGIN = 'https://njupt.hicancan.top';
export const SOCIAL_IMAGE_URL = `${SITE_ORIGIN}/assets/icon-512x512.png`;
export const SOCIAL_IMAGE_ALT = 'njupt-search 标志';

interface PageIdentity {
    title: string;
    description: string;
    pathname: '/' | '/search' | '/community' | '/materials' | '/timetable' | '/classrooms' | '/exam';
}

export interface PageSeo extends PageIdentity {
    indexable: boolean;
    canonical: string | null;
    websiteStructuredData: boolean;
}

const PAGE_IDENTITIES: Record<AppRoute, PageIdentity> = {
    home: {
        title: 'njupt-search｜南邮网站与教务查询',
        description: '搜索南邮校方信息、校园经验和课程资料，查询班级课表、考试安排与教室空间。',
        pathname: '/',
    },
    search: {
        title: '南邮网站搜索｜njupt-search',
        description: '搜索南京邮电大学各网站的通知、附件和办事信息。',
        pathname: '/search',
    },
    community: {
        title: '南邮社区搜索｜njupt-search',
        description: '搜索南邮生存手册中的校园生活、学习经验和办事指南。',
        pathname: '/community',
    },
    materials: {
        title: '南邮资料搜索｜njupt-search',
        description: '搜索南京邮电大学课程资料、历年试卷、课件和复习笔记。',
        pathname: '/materials',
    },
    exam: {
        title: '南邮考试安排查询｜njupt-search',
        description: '输入班级号，查询南京邮电大学考试时间、地点和考场，并可导出日历。',
        pathname: '/exam',
    },
    timetable: {
        title: '南邮班级课表查询｜njupt-search',
        description: '输入班级号，按周查看南京邮电大学课程时间、教师和教室，并可导出日历。',
        pathname: '/timetable',
    },
    classrooms: {
        title: '南邮教室空间浏览｜njupt-search',
        description: '按校区、楼栋和楼层浏览南京邮电大学教室，并查看所选时段的课程与考试占用。',
        pathname: '/classrooms',
    },
};

const escapeHtmlAttribute = (value: string): string => value
    .replace(/&/g, '&amp;')
    .replace(/"/g, '&quot;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');

export const resolvePageSeo = (route: AppRoute, hasQueryParams = false): PageSeo => {
    const identity = PAGE_IDENTITIES[route];
    const indexable = !hasQueryParams;
    return {
        ...identity,
        indexable,
        canonical: indexable ? `${SITE_ORIGIN}${identity.pathname}` : null,
        websiteStructuredData: route === 'home' && indexable,
    };
};

const websiteStructuredData = JSON.stringify({
    '@context': 'https://schema.org',
    '@type': 'WebSite',
    name: 'njupt-search',
    alternateName: ['南邮搜索', 'NJUPT Search'],
    url: `${SITE_ORIGIN}/`,
}).replace(/</g, '\\u003c');

const queryNoindexGuard = `<script data-seo-query-policy>(function(){if(!location.search)return;var r=document.querySelector('meta[name="robots"]');if(r)r.setAttribute('content','noindex, follow');var c=document.querySelector('link[rel="canonical"]');if(c)c.remove();var s=document.querySelector('script[type="application/ld+json"]');if(s)s.remove();})();</script>`;

export const renderPageHead = (seo: PageSeo): string => {
    const title = escapeHtmlAttribute(seo.title);
    const description = escapeHtmlAttribute(seo.description);
    const pageUrl = `${SITE_ORIGIN}${seo.pathname}`;
    const tags = [
        `<title>${title}</title>`,
        `<meta name="description" content="${description}" />`,
        `<meta name="robots" content="${seo.indexable ? 'index, follow' : 'noindex, follow'}" />`,
        seo.canonical === `${SITE_ORIGIN}/`
            ? '<meta name="msvalidate.01" content="E44C2F6A09F0A678DED98C8374E549A1" />'
            : '',
        seo.canonical === `${SITE_ORIGIN}/`
            ? '<meta name="baidu-site-verification" content="codeva-3WJsMJj16W" />'
            : '',
        seo.canonical ? `<link rel="canonical" href="${seo.canonical}" />` : '',
        '<meta property="og:site_name" content="njupt-search" />',
        `<meta property="og:title" content="${title}" />`,
        `<meta property="og:description" content="${description}" />`,
        `<meta property="og:url" content="${pageUrl}" />`,
        `<meta property="og:image" content="${SOCIAL_IMAGE_URL}" />`,
        `<meta property="og:image:alt" content="${SOCIAL_IMAGE_ALT}" />`,
        '<meta property="og:locale" content="zh_CN" />',
        '<meta property="og:type" content="website" />',
        '<meta name="twitter:card" content="summary" />',
        `<meta name="twitter:title" content="${title}" />`,
        `<meta name="twitter:description" content="${description}" />`,
        `<meta name="twitter:image" content="${SOCIAL_IMAGE_URL}" />`,
        seo.websiteStructuredData
            ? `<script type="application/ld+json">${websiteStructuredData}</script>`
            : '',
        seo.indexable ? queryNoindexGuard : '',
    ].filter(Boolean);
    return `<!-- njupt-seo:start -->\n${tags.join('\n')}\n<!-- njupt-seo:end -->`;
};

const setNamedMeta = (name: string, content: string): void => {
    let element = document.head.querySelector<HTMLMetaElement>(`meta[name="${name}"]`);
    if (!element) {
        element = document.createElement('meta');
        element.name = name;
        document.head.append(element);
    }
    element.content = content;
};

const setPropertyMeta = (property: string, content: string): void => {
    let element = document.head.querySelector<HTMLMetaElement>(`meta[property="${property}"]`);
    if (!element) {
        element = document.createElement('meta');
        element.setAttribute('property', property);
        document.head.append(element);
    }
    element.content = content;
};

export const applyPageSeo = (seo: PageSeo): void => {
    document.title = seo.title;
    setNamedMeta('description', seo.description);
    setNamedMeta('robots', seo.indexable ? 'index, follow' : 'noindex, follow');
    setPropertyMeta('og:site_name', 'njupt-search');
    setPropertyMeta('og:title', seo.title);
    setPropertyMeta('og:description', seo.description);
    setPropertyMeta('og:url', `${SITE_ORIGIN}${seo.pathname}`);
    setPropertyMeta('og:image', SOCIAL_IMAGE_URL);
    setPropertyMeta('og:image:alt', SOCIAL_IMAGE_ALT);
    setPropertyMeta('og:locale', 'zh_CN');
    setPropertyMeta('og:type', 'website');
    setNamedMeta('twitter:card', 'summary');
    setNamedMeta('twitter:title', seo.title);
    setNamedMeta('twitter:description', seo.description);
    setNamedMeta('twitter:image', SOCIAL_IMAGE_URL);

    const canonical = document.head.querySelector<HTMLLinkElement>('link[rel="canonical"]');
    if (seo.canonical) {
        const element = canonical ?? document.createElement('link');
        element.rel = 'canonical';
        element.href = seo.canonical;
        if (!canonical) document.head.append(element);
    } else {
        canonical?.remove();
    }

    const structuredData = document.head.querySelector<HTMLScriptElement>('script[type="application/ld+json"]');
    if (seo.websiteStructuredData) {
        const element = structuredData ?? document.createElement('script');
        element.type = 'application/ld+json';
        element.textContent = websiteStructuredData;
        if (!structuredData) document.head.append(element);
    } else {
        structuredData?.remove();
    }
};
