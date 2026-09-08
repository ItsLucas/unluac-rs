local function run(outer_count, inner_count, stop)
  local selected = -1
  local total = 0
  for outer = 1, outer_count do
    for inner = 1, inner_count do
      selected = outer * 10 + inner
      if inner == stop then
        selected = -selected
        break
      end
    end
    total = total + selected
    if outer == stop then
      break
    end
  end
  return selected, total
end

for outer_count = 0, 3 do
  for inner_count = 0, 3 do
    for stop = 0, 4 do
      print(outer_count, inner_count, stop, run(outer_count, inner_count, stop))
    end
  end
end
