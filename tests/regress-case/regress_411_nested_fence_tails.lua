local function run(count, mode, skip)
  local trace = {}
  local function probe(label, value)
    trace[#trace + 1] = label
    return value
  end
  local values = { 1, 2, 3 }
  for outer = 1, count do
    repeat
      if outer == skip then
        trace[#trace + 1] = "skip:" .. outer
        break
      end
      for index, value in ipairs(values) do
        if probe("first", mode) == 1 or probe("second", mode) == 2 then
          trace[#trace + 1] = "yes:" .. index .. ":" .. value
        else
          trace[#trace + 1] = "no:" .. index .. ":" .. value
        end
      end
      trace[#trace + 1] = "outer-tail:" .. outer
    until true
    trace[#trace + 1] = "after-repeat:" .. outer
  end
  return table.concat(trace, ",")
end

for count = 0, 3 do
  for mode = 0, 2 do
    for skip = 0, 3 do
      print(count, mode, skip, run(count, mode, skip))
    end
  end
end
