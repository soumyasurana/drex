import type { Metadata } from 'next';
import './globals.css';
import { Providers } from '@/components/providers';

export const metadata: Metadata = {
  title: 'Contextra — AI Context Engineering Platform',
  description: 'Production-ready AI context engineering platform written in Rust.',
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en" className="dark h-full antialiased">
      <body className="min-h-full flex flex-col bg-[#090a0f] text-zinc-100 font-sans">
        <Providers>{children}</Providers>
      </body>
    </html>
  );
}
