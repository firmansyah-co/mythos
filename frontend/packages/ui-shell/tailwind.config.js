/** @type {import('tailwindcss').Config} */
export default {
  content: [
    './index.html',
    './src/**/*.{js,ts,jsx,tsx}',
    '../../plugins/*/src/**/*.{js,ts,jsx,tsx}',
  ],
  theme: {
    extend: {
      colors: {
        scada: {
          base: '#0A0D12',
          surface: '#0F141C',
          elevated: '#161E2E',
          alarmRed: '#DC2626',
          warningAmber: '#EA580C',
          advisoryYellow: '#CA8A04',
          normalGreen: '#16A34A',
          cyan: '#06B6D4',
        },
      },
    },
  },
  plugins: [],
};
