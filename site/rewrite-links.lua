-- Pandoc Lua filter used when rendering README.md into the landing page.
--
-- The README links to the mdBook *sources* (`docs/index.md`,
-- `docs/architecture.md`, `docs/decisions`). On the built website the book
-- lives under `./docs/` as HTML, so rewrite those relative links to point at
-- the generated pages. External links, anchors and everything outside `docs/`
-- are left untouched.

local function rewrite(target)
  if not target or target == "" then
    return target
  end
  -- Leave absolute URLs (http:, https:, mailto:, ...) alone.
  if target:match("^%a[%w+.-]*:") then
    return target
  end

  local path, fragment = target, ""
  local hash = target:find("#", 1, true)
  if hash then
    path = target:sub(1, hash - 1)
    fragment = target:sub(hash)
  end

  if path:match("^docs/") then
    if path:sub(-3) == ".md" then
      path = path:sub(1, -4) .. ".html"      -- docs/foo.md -> docs/foo.html
    elseif path:match("^docs/decisions/?$") then
      path = "docs/"                          -- the book has no decisions index
    end
    return "./" .. path .. fragment
  end

  return target
end

function Link(el)
  el.target = rewrite(el.target)
  return el
end
