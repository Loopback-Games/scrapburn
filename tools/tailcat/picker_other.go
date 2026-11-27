//go:build !linux

package main

import (
	"context"
	"errors"
	"os/exec"
	"runtime"
	"strings"
)

func invitePath(ctx context.Context, save bool) (string, error) {
	var command *exec.Cmd
	switch runtime.GOOS {
	case "darwin":
		dialog := "choose file with prompt \"Open a Scrapburn invite\""
		if save {
			dialog = "choose file name with prompt \"Save a private invite\" default name \"Scrapburn Race.scrapburn\""
		}
		command = exec.CommandContext(ctx, "/usr/bin/osascript", "-e", "try", "-e", "return POSIX path of ("+dialog+")", "-e", "on error number -128", "-e", "return \"\"", "-e", "end try")
	case "windows":
		kind := "OpenFileDialog"
		if save {
			kind = "SaveFileDialog"
		}
		script := "Add-Type -AssemblyName System.Windows.Forms; [Console]::OutputEncoding=[Text.UTF8Encoding]::new(); $dlg=New-Object System.Windows.Forms." + kind + "; $dlg.Filter='Scrapburn invite (*.scrapburn)|*.scrapburn|All files (*.*)|*.*';"
		if save {
			script += "$dlg.FileName='Scrapburn Race.scrapburn';"
		}
		command = exec.CommandContext(ctx, "powershell.exe", "-NoProfile", "-STA", "-Command", script+"if ($dlg.ShowDialog() -eq 'OK') { [Console]::Write($dlg.FileName) }")
	default:
		return "", errors.New("unsupported desktop")
	}
	out, err := command.Output()
	return strings.TrimRight(string(out), "\r\n"), err
}
