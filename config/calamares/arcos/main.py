#!/usr/bin/env python3

import subprocess

import libcalamares


def pretty_name():
    return "Applying the ArcOS profile."


def run():
    root = libcalamares.globalstorage.value("rootMountPoint")
    username = libcalamares.globalstorage.value("username")
    if not root or not username:
        return (
            "ArcOS profile could not be applied",
            "The target root or selected username is missing.",
        )

    try:
        subprocess.run(
            ["@finalizeScript@", root, username],
            check=True,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
        )
    except subprocess.CalledProcessError as error:
        output = error.stdout or "ArcOS finalization failed without output."
        libcalamares.utils.error(output)
        return ("ArcOS finalization failed", output)

    return None
