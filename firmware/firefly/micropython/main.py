"""
Firefly OLED - Main Entry Point

ESP8266 NodeMCU with 128×64 SSD1306 OLED display (dual-color: yellow header, blue body).
Serial protocol for communication with Moss Firefly companion.

Display Layout:
  - Yellow zone (header): rows 0-15, shows stone name with auto-scroll if long
  - Blue zone (body): rows 16-63, shows health icon, CPU/MEM bars, uptime

Protocol Commands (line-based, \\n terminated):
  I                    - Device info (returns: OK,firefly-oled,esp8266,128x64,...)
  C                    - Clear display
  S,name               - Set stone name (uppercased, auto-scrolls if > 120px)
  H,state              - Set health (thriving/withering/wilting/resting)
  M,cpu,mem,uptime     - Update metrics (cpu/mem: 0-100, uptime: string like "1h")
  T,x,y,text           - Draw text at pixel position
  R                    - Refresh/redraw status screen
  B,0-100              - Set brightness (0-100 mapped to 0-255 contrast)

  Drawing Commands:
  FILL,0|1             - Fill display with 0 (off) or 1 (on)
  RECT,x,y,w,h[,fill]  - Draw rectangle (fill=1 for filled)
  BAR,x,y,w,h,percent  - Draw progress bar

  Animation Commands:
  WIPE-IN,line1,line2  - Wipe left->right with message, then clear to status
  WIPE-OUT,line1,line2 - Wipe right->left with message, then clear to status
  BLINK,count          - Blink blue zone (default: 3 blinks)
  PULSE,count          - Pulse brightness (breathing effect, default: 3 pulses)

Responses:
  OK                   - Command succeeded
  OK,data              - Command succeeded with data
  ERR,message          - Command failed

Notes:
  - Display refreshes automatically at 4Hz (250ms timer)
  - Header scrolling uses tick-based "shader" approach
  - Long stone names pause at each end, then scroll smoothly
  - Animations pause the refresh timer while running
"""

import gc
gc.collect()
import sys
import time
import select
from machine import Timer
gc.collect()
from firefly_oled import FireflyOLED
gc.collect()

# Initialize display
display = None
refresh_timer = None
needs_refresh = False  # Flag set by timer

# Animation queue: (type, args) or None for idle
pending_animation = None

# Use UART directly for unbuffered output
from machine import UART
_uart = UART(0, 115200)


def respond(msg):
    """Send response immediately via UART write (unbuffered).

    Using UART.write() directly ensures bytes are sent immediately
    to the hardware TX buffer. The small delay ensures the hardware
    TX FIFO is flushed before we return.
    """
    _uart.write(msg + "\n")
    # Brief delay to ensure hardware UART TX buffer flushes
    # ESP8266 UART at 115200 baud: ~0.1ms per byte, 3 bytes = 0.3ms
    # Add 1ms margin to be safe
    time.sleep_ms(2)


def timer_callback(t):
    """Timer callback - just sets refresh flag."""
    global needs_refresh
    needs_refresh = True


def init_display():
    """Initialize the OLED display."""
    global display, refresh_timer
    try:
        display = FireflyOLED()
        display.draw_boot_screen()
        time.sleep(1)
        # Start 200ms refresh timer (5Hz) - just sets flag
        refresh_timer = Timer(-1)
        refresh_timer.init(period=200, mode=Timer.PERIODIC, callback=timer_callback)
        return True
    except Exception as e:
        respond(f"ERR,display_init:{e}")
        return False


def run_animation(anim_type, anim_args):
    """Run an animation (blocking)."""
    global display
    if anim_type == "wipe-in":
        display.wipe(anim_args[0], anim_args[1], direction="in")
    elif anim_type == "wipe-out":
        display.wipe(anim_args[0], anim_args[1], direction="out")
    elif anim_type == "blink":
        display.blink(count=anim_args)
    elif anim_type == "pulse":
        display.pulse(count=anim_args)


def parse_command(line):
    """Parse and execute a command."""
    global display, pending_animation

    line = line.strip()
    if not line:
        return

    # Split command and args
    parts = line.split(",", 1)
    cmd = parts[0].upper()
    args = parts[1] if len(parts) > 1 else ""

    try:
        if cmd == "I":
            # Device info
            respond(display.device_info())

        elif cmd == "C":
            # Clear display
            display.clear()
            respond("OK")

        elif cmd == "S":
            # Set stone name
            display.set_stone_name(args)
            display.draw_status_screen()
            respond("OK")

        elif cmd == "H":
            # Set health state
            display.set_health(args.lower())
            display.draw_status_screen()
            respond("OK")

        elif cmd == "M":
            # Update metrics: cpu,mem,uptime (timer handles refresh)
            metric_parts = args.split(",")
            cpu = int(metric_parts[0]) if len(metric_parts) > 0 else None
            mem = int(metric_parts[1]) if len(metric_parts) > 1 else None
            uptime = metric_parts[2] if len(metric_parts) > 2 else None
            display.update_metrics(cpu=cpu, mem=mem, uptime=uptime)
            respond("OK")

        elif cmd == "T":
            # Draw text: x,y,text
            text_parts = args.split(",", 2)
            if len(text_parts) >= 3:
                x = int(text_parts[0])
                y = int(text_parts[1])
                text = text_parts[2]
                display.text(text, x, y)
                display.show()
                respond("OK")
            else:
                respond("ERR,invalid_args")

        elif cmd == "R":
            # Refresh status screen
            display.draw_status_screen()
            respond("OK")

        elif cmd == "B":
            # Set brightness (contrast)
            contrast = int(args)
            # Map 0-100 to 0-255
            contrast_byte = int(contrast * 255 / 100)
            display.oled.contrast(contrast_byte)
            respond("OK")

        elif cmd == "FILL":
            # Fill display (for testing)
            value = int(args) if args else 1
            display.fill(value)
            respond("OK")

        elif cmd == "RECT":
            # Draw rectangle: x,y,w,h,fill
            rect_parts = args.split(",")
            if len(rect_parts) >= 4:
                x = int(rect_parts[0])
                y = int(rect_parts[1])
                w = int(rect_parts[2])
                h = int(rect_parts[3])
                fill = int(rect_parts[4]) if len(rect_parts) > 4 else 0
                if fill:
                    display.draw_fill_rect(x, y, w, h)
                else:
                    display.draw_rect(x, y, w, h)
                display.show()
                respond("OK")
            else:
                respond("ERR,invalid_args")

        elif cmd == "BAR":
            # Draw progress bar: x,y,w,h,percent
            bar_parts = args.split(",")
            if len(bar_parts) >= 5:
                x = int(bar_parts[0])
                y = int(bar_parts[1])
                w = int(bar_parts[2])
                h = int(bar_parts[3])
                percent = int(bar_parts[4])
                display.draw_progress_bar(x, y, w, h, percent)
                display.show()
                respond("OK")
            else:
                respond("ERR,invalid_args")

        # ==================== ANIMATION COMMANDS ====================
        # Animations are queued and run by the timer callback

        elif cmd == "WIPE-IN":
            # Wipe left→right transition
            wipe_parts = args.split(",", 1)
            line1 = wipe_parts[0] if len(wipe_parts) > 0 else ""
            line2 = wipe_parts[1] if len(wipe_parts) > 1 else ""
            pending_animation = ("wipe-in", (line1, line2))
            respond("OK")

        elif cmd == "WIPE-OUT":
            # Wipe right→left transition
            wipe_parts = args.split(",", 1)
            line1 = wipe_parts[0] if len(wipe_parts) > 0 else ""
            line2 = wipe_parts[1] if len(wipe_parts) > 1 else ""
            pending_animation = ("wipe-out", (line1, line2))
            respond("OK")

        elif cmd == "BLINK":
            # Blink blue zone
            count = int(args) if args else 3
            pending_animation = ("blink", count)
            respond("OK")

        elif cmd == "PULSE":
            # Pulse brightness
            count = int(args) if args else 3
            pending_animation = ("pulse", count)
            respond("OK")

        else:
            respond(f"ERR,unknown_cmd:{cmd}")

    except Exception as e:
        respond(f"ERR,{e}")


def main():
    """Main loop - initialize and process commands."""
    global needs_refresh, pending_animation

    respond("Firefly OLED starting...")

    if not init_display():
        respond("ERR,failed_to_init_display")
        return

    # Show initial status screen
    display.set_stone_name("Connecting...")
    display.set_health("resting")
    display.draw_status_screen()

    respond("OK,ready")

    # Setup poll for non-blocking stdin
    poll = select.poll()
    poll.register(sys.stdin, select.POLLIN)

    # Main loop: check for commands and handle display updates
    while True:
        try:
            # Check for pending animation first (priority)
            if pending_animation:
                anim_type, anim_args = pending_animation
                pending_animation = None
                needs_refresh = False  # Clear flag during animation
                run_animation(anim_type, anim_args)

            # Check if timer says we need a refresh
            elif needs_refresh:
                needs_refresh = False
                display.draw_status_screen()

            # Non-blocking check for serial input (0ms timeout)
            events = poll.poll(0)
            if events:
                line = sys.stdin.readline()
                if line:
                    parse_command(line)
            else:
                time.sleep_ms(10)  # Small sleep to avoid busy loop

        except KeyboardInterrupt:
            respond("OK,interrupted")
            break
        except Exception as e:
            respond(f"ERR,{e}")


# Auto-run on boot
main()
