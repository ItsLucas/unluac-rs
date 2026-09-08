local function run(mode, count, stop, skip)
  local trace = {}
  repeat
    if mode == 1 then
      for index = 1, count do
        trace[#trace + 1] = index
        if index == stop then
          break
        end
      end
      if skip then
        break
      end
    else
      trace[#trace + 1] = "other"
      if stop == 1 then
        break
      end
    end
    trace[#trace + 1] = "tail"
  until true
  trace[#trace + 1] = "done"
  return table.concat(trace, ",")
end

for mode = 1, 2 do
  for count = 0, 3 do
    for stop = 0, 4 do
      print(mode, count, stop, run(mode, count, stop, false), run(mode, count, stop, true))
    end
  end
end
