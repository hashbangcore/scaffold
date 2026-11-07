local input = (PANDOC_STATE.input_files or {})[1] or ""
local is_spanish = input:match("README%.es%.md$") ~= nil

function Link(el)
    local path = el.target:gsub("^/", "")
    path = path:gsub("^%./", "")

    if path == "LICENSE" then
        el.target = is_spanish and "../license/" or "./license/"
    elseif path == "README.md" then
        el.target = is_spanish and "../" or "./"
    elseif path == "README.es.md" then
        el.target = is_spanish and "./" or "./es/"
    end

    return el
end
