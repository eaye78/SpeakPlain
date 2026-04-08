from PIL import Image, ImageDraw

# 彩虹渐变背景 400x48
img = Image.new('RGBA', (400, 48))
draw = ImageDraw.Draw(img)

colors = [
    (102, 126, 234),   # 蓝紫
    (118, 75, 162),    # 紫色
    (240, 147, 251)    # 粉色
]

for i in range(400):
    x = i / 400.0
    if x < 0.5:
        c1, c2 = colors[0], colors[1]
        t = x * 2
    else:
        c1, c2 = colors[1], colors[2]
        t = (x - 0.5) * 2
    
    r = int(c1[0] * (1 - t) + c2[0] * t)
    g = int(c1[1] * (1 - t) + c2[1] * t)
    b = int(c1[2] * (1 - t) + c2[2] * t)
    
    draw.line([(i, 0), (i, 47)], fill=(r, g, b, 255))

img.save('d:/projects/SpeakPlain/speakplain/skins/rainbow/background.png')
print("彩虹背景图已生成")
