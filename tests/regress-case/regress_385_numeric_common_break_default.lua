local function run(count, stop, flag)
  local selected = "default"
  for index = 1, count do
    if index == stop then
      if flag then
        selected = "first"
      else
        selected = "second"
      end
      break
    end
  end
  return selected
end

for count = 0, 4 do
  for stop = 0, 5 do
    print(count, stop, run(count, stop, false), run(count, stop, true))
  end
end
