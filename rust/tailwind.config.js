/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      colors: {
        bg: {
          900: "#12121a",
          800: "#18181f",
          700: "#22222a",
          600: "#2a2a33",
        },
        accent: {
          DEFAULT: "#64b4ff",
          hi: "#ff50b4",
        },
      },
      fontFamily: {
        sans: [
          "-apple-system",
          "BlinkMacSystemFont",
          "'PingFang SC'",
          "'Microsoft YaHei'",
          "system-ui",
          "sans-serif",
        ],
      },
    },
  },
  plugins: [],
};
