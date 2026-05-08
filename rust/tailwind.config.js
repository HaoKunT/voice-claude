/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./indicator.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      colors: {
        // Raycast 配色
        bg: {
          900: "#0b0b0f",  // 最深背景
          800: "#13131a",  // 卡片背景
          700: "#1c1c25",  // hover
          600: "#26262f",  // 输入框
          500: "#323240",  // 边框
        },
        accent: {
          DEFAULT: "#ff5c5c", // Raycast 招牌红
          hi: "#ff3838",
        },
        brand: {
          purple: "#9b87f5",
          blue: "#7dd3fc",
        },
      },
      fontFamily: {
        sans: [
          "-apple-system",
          "BlinkMacSystemFont",
          "'SF Pro Text'",
          "'SF Pro Display'",
          "'PingFang SC'",
          "'Microsoft YaHei'",
          "system-ui",
          "sans-serif",
        ],
      },
      boxShadow: {
        card: "0 4px 24px rgba(0, 0, 0, 0.3)",
        glow: "0 0 20px rgba(155, 135, 245, 0.3)",
      },
      backdropBlur: {
        heavy: "40px",
      },
    },
  },
  plugins: [],
};
