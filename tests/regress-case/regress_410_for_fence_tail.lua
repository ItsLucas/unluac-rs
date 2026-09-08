local function run(count, mode)
  local trace = {}
  local function probe(label, value)
    trace[#trace + 1] = label
    return value
  end
  for index = 1, count do
    if probe("first", mode) == 1 or probe("second", mode) == 2 then
      trace[#trace + 1] = "yes:" .. index
    else
      trace[#trace + 1] = "no:" .. index
    end
  end
  trace[#trace + 1] = "after"
  return table.concat(trace, ",")
end

for count = 0, 3 do
  for mode = 0, 2 do
    print(count, mode, run(count, mode))
  end
end
