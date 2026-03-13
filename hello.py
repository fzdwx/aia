#!/usr/bin/env python3
# -*- coding: utf-8 -*-

"""
Hello World 升级版 - 交互式问候程序
包含：姓名输入、时间显示、等友好功能
"""

from datetime import datetime
import random


def get_greeting():
    """根据当前时间返回不同问候语"""
    hour = datetime.now().hour
    if 5 <= hour < 12:
        return "早上好"
    elif 12 <= hour < 18:
        return "下午好"
    elif 18 <= hour < 23:
        return "晚上好"
    else:
        return "夜深了"


def show_ascii_art():
    """显示 ASCII 艺术"""
    art = [
        """
    ╭─────────╮
    │ HELLO ♡ │
    ╰─────────╯
    """,
        r"""
    (\_/)
   ( o.o )
    > ^ <
   HELLO!
    """,
        """
      ★
     ╱│╲
    / │ \
   ═══════
    WORLD
    """
    ]
    print(random.choice(art))


def main():
    """主函数"""
    print("\n" + "=" * 40)
    print("   🎉 欢迎来到 Python 世界！")
    print("=" * 40 + "\n")
    
    # 获取用户名
    name = input("请问你的名字是？ → ")
    if not name.strip():
        name = "朋友"
    
    # 个性化打招呼
    greeting = get_greeting()
    print(f"\n{greeting}，{name}！很高兴见到你！ 🌟\n")
    
    # 交互菜单
    while True:
        print("\n" + "-" * 30)
        print("  📋 菜单选项")
        print(" " * 4 + "1️⃣  再说一次你好")
        print(" " * 4 + "2️⃣  显示当前时间")
        print(" " * 4 + "3️⃣  看个小动画")
        print(" " * 4 + "4️⃣  查看问候历史")
        print(" " * 4 + "0️⃣  退出程序")
        print("-" * 30)
        
        choice = input("\n请选择一个选项 (0-4) → ").strip()
        
        if choice == "1":
            greeting = get_greeting()
            print(f"\n{greeting}，{name}！今天也要加油哦 💪\n")
            
        elif choice == "2":
            now = datetime.now()
            print(f"\n🕐 当前时间：{now.strftime('%Y年%m月%d日 %H:%M:%S')}")
            print(f"   今天是 {now.strftime('%A')}（星期{['一','二','三','四','五','六','日'][now.weekday()]}）\n")
            
        elif choice == "3":
            show_ascii_art()
            
        elif choice == "4":
            print(f"\n📌 记住啦，对 {name} 说过的问候：")
            print(f"   最近一次是：{get_greeting()}，{name}！\n")
            
        elif choice == "0":
            print(f"\n👋 再见，{name}！祝你有美好的一天！")
            print("   程序已退出。\n")
            break
            
        else:
            print("\n⚠️ 无效的选项，请重新输入 (0-4) \n")


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\n\n程序被用户中断（按下了 Ctrl+C）")
