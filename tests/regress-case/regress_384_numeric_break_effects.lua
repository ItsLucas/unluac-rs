local function run(count, stop)
  local trace = {}
  local function record(value)
    trace[#trace + 1] = value
    return value
  end
  local selected = record(-1)
  for index = record(count), record(1), record(-1) do
    selected = record(index * 10)
    if index == stop then
      selected = record(-index)
      break
    end
    record(index)
  end
  return selected, table.concat(trace, ",")
end

for count = 0, 4 do
  for stop = 0, 5 do
    print(count, stop, run(count, stop))
  end
end
