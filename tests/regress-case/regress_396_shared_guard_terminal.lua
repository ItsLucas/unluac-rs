local function choose(a, b, tail)
  local trace = {}
  local function probe(label, value)
    trace[#trace + 1] = label
    return value
  end
  local result
  if ((a and b) or (not a and not b)) and not probe("tail", tail) then
    result = "selected"
  else
    result = "skipped"
  end
  if ((not a or not b) and (a or b)) or probe("inverse", tail) then
    result = result .. ":inverse"
  end
  local value = (probe("a", a) or probe("b", b)) and probe("value", tail)
  return result, type(value), tostring(value), table.concat(trace, ",")
end

for left = 0, 3 do
  for right = 0, 3 do
    local a = ({ false, true, 0, "" })[left + 1]
    local b = ({ false, true, 0, "" })[right + 1]
    print(left, right, choose(a, b, false))
    print(left, right, choose(a, b, "result"))
    print(left, right, choose(a, b, nil))
  end
end
print(choose(nil, nil, false))
print(choose(nil, true, false))
print(choose(true, nil, false))
