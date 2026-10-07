import { createRootRoute, Link, Outlet } from '@tanstack/react-router'

export const Route = createRootRoute({
  component: Outlet,
  notFoundComponent: () => (
    <div className="grid min-h-full place-items-center bg-[#F4F4F5] p-6 text-center text-[#1A1A1E]">
      <div className="space-y-2">
        <p className="text-lg font-semibold">Page not found</p>
        <Link to="/" className="text-sm underline underline-offset-4">
          Back to alashi
        </Link>
      </div>
    </div>
  ),
})
