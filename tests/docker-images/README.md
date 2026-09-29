# Tests Codex dans Docker

L'image fournit un environnement Codex vierge et reproductible pour tester
RosettAI. Elle contient la CLI Codex, le binaire `rai`, Git et ripgrep. Elle
s'exécute avec un utilisateur non privilégié.

## Construire l'image

```sh
docker build -f tests/docker-images/Dockerfile.codex -t rosettai-codex:0.158.0 .
```

Pour construire une autre version de Codex :

```sh
docker build \
  --build-arg CODEX_VERSION=0.158.0 \
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
```
