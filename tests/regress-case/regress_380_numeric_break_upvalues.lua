local function capture(count, stop)
  local selected = function() return -1 end
  local saved = {}
  for index = 1, count do
    local value = index
    selected = function() return value, index end
    saved[#saved + 1] = selected
    if index == stop then
      value = -value
      break
    end
    value = value + 20
  end
  print("selected", selected())
  for index = 1, #saved do
    print("saved", index, saved[index]())
  end
end

for count = 0, 4 do
  for stop = 0, 5 do
    capture(count, stop)
  end
end
