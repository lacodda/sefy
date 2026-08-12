// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// https://astro.build/config
export default defineConfig({
	site: 'https://lacodda.github.io',
	base: '/sefy',
	integrations: [
		starlight({
			title: 'sefy',
			description: 'An inconspicuous encrypted store for notes, credentials and files: a vault whose file looks like nothing.',
			logo: {
				src: './src/assets/logo.svg',
				alt: 'sefy',
			},
			favicon: '/favicon.svg',
			customCss: ['./src/styles/brand.css'],
			head: [
				{ tag: 'link', attrs: { rel: 'apple-touch-icon', href: '/sefy/apple-touch-icon.png' } },
				{ tag: 'meta', attrs: { property: 'og:image', content: 'https://raw.githubusercontent.com/lacodda/sefy/main/assets/social-preview.png' } },
				{ tag: 'meta', attrs: { name: 'twitter:card', content: 'summary_large_image' } },
			],
			social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/lacodda/sefy' }],
			editLink: {
				baseUrl: 'https://github.com/lacodda/sefy/edit/main/docs/',
			},
			sidebar: [
				{ label: 'Getting Started', slug: 'getting-started' },
				{
					label: 'Guides',
					items: [{ autogenerate: { directory: 'guides' } }],
				},
				{
					label: 'Concepts',
					items: [{ autogenerate: { directory: 'concepts' } }],
				},
				{
					label: 'Reference',
					items: [{ autogenerate: { directory: 'reference' } }],
				},
			],
		}),
	],
});
