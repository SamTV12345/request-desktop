/** @type {import('tailwindcss').Config} */
export default {
    content: [
        "./index.html",
        "./src/**/*.{js,ts,jsx,tsx}",
    ],
    theme: {
        extend: {
            colors: {
                border_strong: 'var(--color-border-strong)',
                background_tertiary: 'var(--background-color-tertiary)',
                basecol: '#262626',
                mustard: {
                    100: '#fae5c0',
                    200: '#fad490',
                    300: '#fac463',
                    400: '#f3ae34',
                    500: '#e69a13',
                    600: '#c07c03',
                    700: '#855602',
                    800: '#4e3201',
                    900: '#271901'
                }
            }
        },
    },
    plugins: [],
}
