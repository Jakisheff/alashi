import { createRootRoute, Link, Outlet } from '@tanstack/react-router'

const Message = ({ title, children }: { title: string; children?: React.ReactNode }) => (
  <div className="grid min-h-full place-items-center bg-[#F4F4F5] p-6 text-center text-[#1A1A1E]">
    <div className="space-y-2">
      <p className="text-lg font-semibold">{title}</p>
      {children}
      <Link to="/" reloadDocument className="text-sm underline underline-offset-4">
        Back to alashi
      </Link>
    </div>
  </div>
)

export const Route = createRootRoute({
  component: Outlet,
  notFoundComponent: () => <Message title="Page not found" />,
  // Any render error below (a changed API response, a failed chunk) lands here instead of the router's default screen
  errorComponent: ({ error }) => (
    <Message title="Something went wrong">
      <p className="max-w-sm text-sm text-[#5f5f68]">{error instanceof Error ? error.message : 'Unknown error'}</p>
    </Message>
  ),
})
