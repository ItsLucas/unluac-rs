local function accumulate(count, stop)
  local total, last = 7, -1
  for index = 1, count do
    total = total + index
    last = index * 3
    if index == stop then
      total = total * 10
      last = -index
      break
    end
    total = total + 2
  end
  return total, last
end

for count = 0, 4 do
  for stop = 0, 5 do
    print(count, stop, accumulate(count, stop))
  end
end
