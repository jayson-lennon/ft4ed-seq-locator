build:
    rsync -ahx favicon/output/ .
    parcel build --public-url /tool/ft4ed --no-source-maps index.pug
    rsync -ahxv favicon/output/ dist

