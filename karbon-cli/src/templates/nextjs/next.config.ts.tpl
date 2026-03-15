import type { NextConfig } from 'next';

const nextConfig: NextConfig = {
  async rewrites() {
    return [
      {
        source: '/api/:path*',
        destination: 'http://localhost:3005/api/:path*',
      },
      {
        source: '/files/:path*',
        destination: 'http://localhost:3005/files/:path*',
      },
    ];
  },
};

export default nextConfig;
