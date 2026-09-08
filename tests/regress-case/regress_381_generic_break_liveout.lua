local function choose(count, stop)
  local calls = 0
  local function iterator(limit, previous)
    calls = calls + 1
    local index = previous + 1
    if index <= limit then
      return index, index * 4
    end
  end
  local selected, sum = -1, 3
  for index, value in iterator, count, 0 do
    selected = value
    sum = sum + value
    if index == stop then
      selected = -value
      break
    end
  end
  return selected, sum, calls
end

for count = 0, 4 do
  for stop = 0, 5 do
    print(count, stop, choose(count, stop))
  end
end
