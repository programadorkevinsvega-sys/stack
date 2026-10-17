// Main function - serves as the entry point for Supabase Edge Functions
serve(async (req: Request) => {
  return new Response(
    JSON.stringify({ status: "ok", message: "Edge Functions running" }),
    { headers: { "Content-Type": "application/json" } }
  );
});
