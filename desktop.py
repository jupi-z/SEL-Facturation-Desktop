"""Point d'entrée desktop : lance le serveur local et ouvre le navigateur."""
from __future__ import annotations
import logging
from logging.handlers import RotatingFileHandler
from socket import socket, AF_INET, SOCK_STREAM
from urllib.request import urlopen
from urllib.error import URLError
import threading
import time
import webbrowser

import db


def existing_app(port=8765):
    try:
        with urlopen(f'http://127.0.0.1:{port}/health',timeout=1) as r:
            import json
            return json.load(r).get('app')=='SEL-Facturation-Local'
    except (URLError,TimeoutError,ValueError,OSError):
        return False


def open_browser():
    for _ in range(45):
        if existing_app():
            webbrowser.open('http://127.0.0.1:8765/')
            return
        time.sleep(.2)


def main():
    d=db.home()
    d.mkdir(parents=True,exist_ok=True)
    logging.basicConfig(level=logging.INFO,handlers=[RotatingFileHandler(d/'application.log',maxBytes=1024*1024,backupCount=2,encoding='utf-8')])
    if existing_app():
        webbrowser.open('http://127.0.0.1:8765/')
        return
    with socket(AF_INET,SOCK_STREAM) as probe:
        if probe.connect_ex(('127.0.0.1',8765))==0:
            raise RuntimeError('Port local 8765 utilisé par une autre application. Fermer celle-ci puis réessayer.')
    from app import create_app
    from waitress import serve
    flask_app=create_app()
    threading.Thread(target=open_browser,daemon=True).start()
    serve(flask_app,host='127.0.0.1',port=8765,threads=4,ident='SEL-Facturation')


if __name__=='__main__': main()
