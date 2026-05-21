// Tailwind v4:PostCSS plugin 拆到 @tailwindcss/postcss,原来的 tailwindcss
// 主包不再做 PostCSS 工作。autoprefixer 仍然单独留着处理 ::-webkit-* 等私有前缀。
export default {
  plugins: {
    "@tailwindcss/postcss": {},
    autoprefixer: {},
  },
};
