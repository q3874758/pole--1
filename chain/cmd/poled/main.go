package main

import (
	"os"

	svrcmd "github.com/cosmos/cosmos-sdk/server/cmd"

	"pole/chain/app"
	"pole/chain/cmd/poled/cmd"
)

func main() {
	rootCmd := cmd.NewRootCmd()
	if err := svrcmd.Execute(rootCmd, "POLED", app.DefaultNodeHome); err != nil {
		os.Exit(1)
	}
}
