# Tests Codex et GitHub Copilot dans Docker

## Lancer les tests d'intégration dans Docker

Depuis la racine du dépôt :

```sh
docker build -f tests/docker-images/Dockerfile.tests -t rosettai-tests:local .
docker run --rm rosettai-tests:local
```

Cette image exécute uniquement les cinq suites d'intégration de `tests/`,
avec leurs fixtures et clones Git. Elle ne lance pas les tests unitaires de
`src/`. Elle fournit un substitut de `codex --version`
pour que les projections de tous les adaptateurs soient testées sans installer
les applications ni utiliser de compte. Le code source est copié à la
construction : reconstruisez l'image après une modification. Les branches de
tests propres à macOS et Windows ne s'exécutent pas dans ce conteneur Linux.

## Lancer les tests unitaires dans une image distincte

```sh
docker build -f tests/docker-images/Dockerfile.unit -t rosettai-unit:local .
docker run --rm rosettai-unit:local
```

Cette image exécute uniquement les tests unitaires du binaire Rust (`src/`).
Elle ne lance aucune des suites d'intégration de `tests/`.

L'image ci-dessous sert aux tests de chargement des vrais harnesses ; elle est
distincte de l'image de la suite Rust.

L'image fournit un environnement reproductible avec Codex CLI, GitHub Copilot
CLI, VS Code Linux et son extension Copilot intégrée, ainsi que le binaire
`rai`, Git et ripgrep. Elle s'exécute avec un utilisateur non privilégié.

## Construire l'image

```sh
docker build -f tests/docker-images/Dockerfile.codex -t rosettai-codex:0.158.0 .
```

Pour construire une autre version de Codex :

```sh
docker build \
  --build-arg CODEX_VERSION=0.158.0 \
  --build-arg COPILOT_VERSION=1.0.89 \
  -f tests/docker-images/Dockerfile.codex \
  -t rosettai-codex:0.158.0 .
```

## Tester la projection

Le test lancé par défaut crée un dépôt temporaire, ajoute une règle et un skill,
exécute `rai sync`, puis vérifie les projections Codex et la conservation du skill
canonique :

```sh
docker run --rm rosettai-codex:0.158.0
```

Ce test ne consomme aucune requête API.

## Tester GitHub Copilot

Le test statique vérifie que Copilot CLI et VS Code sont installés, que Copilot
Chat est inclus dans VS Code, puis que `rai sync` produit les instructions et
l'agent Copilot attendus :

```sh
docker run --rm rosettai-codex:0.158.0 copilot-smoke
```

Pour vérifier que Copilot CLI charge réellement les instructions projet, passez
un jeton GitHub autorisé pour Copilot à l'exécution :

```sh
docker run --rm -e COPILOT_GITHUB_TOKEN rosettai-codex:0.158.0 copilot-smoke --live
```

Le test live effectue une requête Copilot et vérifie un marqueur présent dans les
instructions projet. Il accepte aussi `GH_TOKEN` ou `GITHUB_TOKEN`.

Pour ouvrir VS Code et Copilot Desktop dans le conteneur, montez le dépôt et
publiez VNC uniquement sur la machine locale :

```sh
docker run --rm -it \
  -p 127.0.0.1:5900:5900 \
  -v "$PWD:/workspace" \
  -v copilot-desktop-config:/home/node/.config/Code \
  rosettai-codex:0.158.0 copilot-desktop
```

Ouvrez `vnc://localhost:5900` avec un client VNC, connectez-vous à GitHub dans
VS Code, puis ouvrez Copilot Chat > Diagnostics pour vérifier les fichiers
projet chargés. Exécutez `rai sync --repo /workspace` avant d'ouvrir le dépôt
si ses projections ne sont pas encore présentes. La connexion VNC n'a pas de
mot de passe : conservez la publication sur `127.0.0.1`.

## Tester le chargement réel du skill

Le test live démarre une vraie session Codex dans le dépôt temporaire. Il demande
explicitement l'utilisation du skill de test et vérifie son marqueur de réponse :

```sh
docker run --rm \
  -e OPENAI_API_KEY \
  rosettai-codex:0.158.0 \
  codex-smoke --live
```

La clé API est transmise uniquement à l'exécution. Le test échoue si Codex ne
renvoie pas exactement `ROSETTAI_SKILL_LOADED`.

## Utiliser l'image manuellement

Passez la clé API uniquement à l'exécution, jamais pendant la construction de
l'image :

```sh
docker run --rm -it \
  -e OPENAI_API_KEY \
  -v "$PWD:/workspace" \
  -v codex-config:/home/node/.codex \
  rosettai-codex:0.158.0 \
  codex
```

La commande monte le dépôt courant dans `/workspace` et conserve la configuration
Codex dans le volume Docker `codex-config`.

Pour exécuter Codex sans interaction :

```sh
docker run --rm \
  -e OPENAI_API_KEY \
  -v "$PWD:/workspace" \
  rosettai-codex:0.158.0 \
  codex exec "Résume ce dépôt"
```

## Vérifier l'installation

```sh
docker run --rm rosettai-codex:0.158.0 codex --version

docker run --rm rosettai-codex:0.158.0 sh -c 'command -v rai'

docker run --rm rosettai-codex:0.158.0 copilot --version

docker run --rm rosettai-codex:0.158.0 code --version
```
