local function select_value(values, wanted)
  local selected = { tag = "empty" }
  for index = 1, #values do
    selected = values[index]
    if selected.tag == wanted then
      break
    end
  end
  return selected.tag
end

local values = { { tag = "first" }, { tag = "second" }, { tag = "last" } }
print(select_value({}, "missing"))
print(select_value(values, "first"))
print(select_value(values, "second"))
print(select_value(values, "missing"))
