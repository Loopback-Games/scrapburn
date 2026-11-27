package main

import (
	"context"
	"crypto/rand"
	"errors"
	"fmt"
	"net/url"

	"github.com/godbus/dbus/v5"
)

func invitePath(ctx context.Context, save bool) (string, error) {
	const service = "org.freedesktop.portal.Desktop"
	const response = "org.freedesktop.portal.Request"
	conn, err := dbus.ConnectSessionBus()
	if err != nil {
		return "", err
	}
	defer conn.Close()
	signals := make(chan *dbus.Signal, 4)
	conn.Signal(signals)
	if err := conn.AddMatchSignal(dbus.WithMatchSender(service), dbus.WithMatchInterface(response), dbus.WithMatchMember("Response")); err != nil {
		return "", err
	}
	token := make([]byte, 16)
	if _, err := rand.Read(token); err != nil {
		return "", err
	}
	opts := map[string]dbus.Variant{"handle_token": dbus.MakeVariant(fmt.Sprintf("scrapburn_%x", token))}
	method, title := "OpenFile", "Join a Scrapburn private race"
	if save {
		method, title = "SaveFile", "Save a private invite to share with friends"
		opts["current_name"] = dbus.MakeVariant("Scrapburn Race.scrapburn")
	}
	var handle dbus.ObjectPath
	err = conn.Object(service, "/org/freedesktop/portal/desktop").CallWithContext(ctx, "org.freedesktop.portal.FileChooser."+method, 0, "", title, opts).Store(&handle)
	if err != nil {
		return "", err
	}
	for {
		select {
		case <-ctx.Done():
			return "", ctx.Err()
		case msg := <-signals:
			if msg == nil {
				return "", errors.New("desktop portal disconnected")
			}
			if msg.Path != handle {
				continue
			}
			var code uint32
			var values map[string]dbus.Variant
			if err := dbus.Store(msg.Body, &code, &values); err != nil {
				return "", err
			}
			if code == 1 {
				return "", nil
			}
			var files []string
			if code != 0 || values["uris"].Store(&files) != nil || len(files) != 1 {
				return "", errors.New("no file selected")
			}
			u, err := url.Parse(files[0])
			if err != nil || u.Scheme != "file" || (u.Host != "" && u.Host != "localhost") || u.Path == "" {
				return "", errors.New("expected local file")
			}
			return u.Path, nil
		}
	}
}
