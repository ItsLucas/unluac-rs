local function run(mode, count, stop)
  local saved = {}
  if mode == 1 then
    for index = 1, count do
      local value = index
      saved[#saved + 1] = function() return value end
      if index == stop then
        if index % 2 == 0 then
          value = -value
        else
          value = value + 10
        end
        break
      end
      value = value + 20
    end
  elseif mode == 2 then
    local value = 99
    saved[#saved + 1] = function() return value end
  else
    for index = 1, count do
      local value = index * 100
      saved[#saved + 1] = function() return value end
      if index == stop then
        value = -value
        break
      end
    end
  end
  for index = 1, #saved do
    print(mode, count, stop, index, saved[index]())
  end
end

for mode = 1, 3 do
  for count = 0, 3 do
    for stop = 0, 4 do
      run(mode, count, stop)
    end
  end
end
