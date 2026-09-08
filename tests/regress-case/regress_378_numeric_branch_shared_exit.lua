local function visit(mode, count, stop, flag)
  local trace = {}
  if mode == 1 then
    for index = 1, count do
      if index == stop then
        trace[#trace + 1] = "a"
        if flag then
          trace[#trace + 1] = "b"
        elseif index > 1 then
          trace[#trace + 1] = "c"
        end
        break
      end
    end
  elseif mode == 2 then
    if flag then
      trace[#trace + 1] = "d"
    else
      trace[#trace + 1] = "e"
    end
  else
    for index = 1, count do
      if index == stop then
        trace[#trace + 1] = "f"
        break
      end
    end
  end
  return table.concat(trace) .. "z"
end

for mode = 1, 3 do
  for count = 0, 3 do
    for stop = 1, 4 do
      print(mode, count, stop, visit(mode, count, stop, false), visit(mode, count, stop, true))
    end
  end
end
