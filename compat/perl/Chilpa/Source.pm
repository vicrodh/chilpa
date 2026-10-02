package Chilpa::Source;

use strict;
use warnings;
use JSON::PP ();
use Digest::SHA qw(sha256_hex);
use Chilpa::Contributions;
use Chilpa::Presentation;
use Chilpa::Metadata;

# How many navigation handles a worker keeps alive at once.
use constant HANDLES => 8192;

sub new {
    bless { nodes => {}, order => [], uses => {}, keyed => {}, sequence => 0,
        session => sha256_hex(join(':', $$, time, rand())) }, $_[0];
}

# Callback references stay inside this worker. Navigation handles expire with
# it; playable identities are the guest's original media URI, never row numbers.
# A node with a stable identity keeps its handle when it is listed again, so
# the host's copy of an earlier listing stays usable; its callback is renewed.
sub remember {
    my ($self, $node, $identity) = @_;
    my $id = defined($identity) && length($identity) ? $self->{keyed}{$identity} : undef;
    $id = undef if defined($id) && !$self->{nodes}{$id};
    $id //= 'menu:' . $self->{session} . ':' . ++$self->{sequence};
    $self->{keyed}{$identity} = $id if defined($identity) && length($identity);
    $self->{nodes}{$id} = $node;
    push @{$self->{order}}, $id;
    $self->{uses}{$id}++;
    while (@{$self->{order}} > HANDLES) {
        my $old = shift @{$self->{order}};
        next if --$self->{uses}{$old} > 0;
        delete $self->{uses}{$old};
        delete $self->{nodes}{$old};
    }
    return $id;
}

sub browse {
    my ($self, $input, $done) = @_;
    return $self->search($input, $done) if ($input->{at} // '') eq 'bridge:search';
    my $node;
    if (length($input->{at} // '')) {
        $node = $self->{nodes}{$input->{at}} or die 'expired navigation handle';
    } else {
        my $feeds = $Chilpa::Contributions::ACTIVE->entries('browse');
        my @keys = sort keys %$feeds;
        die 'no registered source feed' unless @keys;
        if (@keys > 1) {
            return $done->($self->listing({ items => [map {
                { name => $feeds->{$_}{tag} || $_, url => $feeds->{$_}{feed} }
            } @keys] }, $input));
        }
        $node = { url => $feeds->{$keys[0]}{feed} };
    }
    my $offset = $input->{cursor} // '0';
    die 'invalid source cursor' unless $offset =~ /^\d{1,9}$/;
    my $args = { index => 0 + $offset, quantity => 100, search => $input->{text} // '', params => {} };
    my $finished = 0;
    my $finish = sub {
        return if $finished++;
        $done->($self->listing($_[0], $input, $node->{name}, $node));
    };
    if (ref($node->{items}) eq 'ARRAY') {
        my @page = @{$node->{items}};
        my $total = scalar @page;
        @page = splice(@page, $offset, 100);
        return $finish->({%$node, items => \@page, offset => 0 + $offset, total => $total});
    }
    die 'unsupported source menu' unless ref($node->{url}) eq 'CODE';
    my $result = $node->{url}->(undef, $finish, $args, @{$node->{passthrough} || []});
    # Most feeds complete through the callback, including synchronously.
    # A callback's incidental return value is not another menu response.
    $finish->($result) if !$finished && ref($result) eq 'HASH' && ref($result->{items}) eq 'ARRAY';
}

sub search {
    my ($self, $input, $done) = @_;
    my $query = $input->{text} // '';
    die 'invalid search text' if ref($query) || length($query) > 4096;
    my @items;
    my $providers = $Chilpa::Contributions::ACTIVE->entries('search');
    for my $key (sort keys %$providers) {
        my $result = $providers->{$key}{func}->(undef, {search => $query});
        push @items, ref($result) eq 'ARRAY' ? @$result : $result ? ($result) : ();
    }
    $done->($self->listing({ items => \@items }, {}));
}

# Global search visits the registered search groups, not an arbitrary source
# tree. Only persistent audio identities enter Chilpa's cross-source results.
sub query {
    my ($self, $input, $done) = @_;
    my $limit = $input->{limit} // 100;
    die 'invalid search limit' unless !ref($limit) && $limit =~ /^\d+$/ && $limit <= 500;
    my (@pending, @tracks, @albums, @playlists, %seen);
    my $visited = 0;
    my $step;
    my $collect = sub {
        my ($listing, $depth) = @_;
        for my $entry (@{$listing->{entries}}) {
            my $display = $listing->{presentation}{$entry->{id}} || {};
            my $metadata = $display->{metadata} || {};
            if ($entry->{kind} eq 'item' && $entry->{playable}) {
                next if $seen{$entry->{id}}++ || @tracks >= $limit;
                push @tracks, { external_id => $entry->{id}, title => $entry->{title},
                    artist => $metadata->{artist} // '', album => $metadata->{album} // '',
                    isrc => $metadata->{isrc}, musicbrainz_recording_id => $metadata->{musicbrainz_recording_id},
                    quality => Chilpa::Presentation::shared_quality($display->{advertised_quality} || {}),
                    duration_ms => $entry->{duration_ms}, artwork_url => $entry->{image_url} };
            } elsif (($display->{entity} // '') eq 'album') {
                next if $seen{$entry->{id}}++ || @albums >= $limit;
                push @albums, { external_id => $entry->{id}, title => $display->{title},
                    artist => $metadata->{artist} // '', year => $display->{year} || undef,
                    artwork_url => $entry->{image_url} };
            } elsif (($display->{entity} // '') eq 'playlist') {
                next if $seen{$entry->{id}}++ || @playlists >= $limit;
                push @playlists, {external_id => $entry->{id}, name => $display->{title},
                    description => $entry->{description}, artwork_url => $entry->{image_url}};
            } elsif ($depth < 2 && $entry->{kind} eq 'folder' && ($display->{entity} // 'folder') eq 'folder') {
                push @pending, [$entry->{id}, $depth + 1] if @pending < 20;
            }
        }
    };
    $step = sub {
        if (!@pending || $visited++ >= 20) {
            undef $step;
            return $done->({tracks => \@tracks, albums => \@albums, playlists => \@playlists});
        }
        my ($id, $depth) = @{shift @pending};
        $self->browse({at => $id, text => $input->{text}}, sub {
            $collect->($_[0], $depth);
            $step->();
        });
    };
    $self->search($input, sub { $collect->($_[0], 0); $step->() });
}

sub listing {
    my ($self, $feed, $input, $title, $parent) = @_;
    # A folder inside an entity's page that names the same entity ("Albums"
    # under an artist) is a part of that page, not the entity again.
    my $around = ref($parent) eq 'HASH' ? Chilpa::Presentation::describe($parent, 0, 1)->{stable_id} : '';
    $feed = { items => $feed } if ref($feed) eq 'ARRAY';
    die 'unsupported source response' unless ref($feed) eq 'HASH' && ref($feed->{items}) eq 'ARRAY';
    die 'source response is too large' if @{$feed->{items}} > 1000;
    my (@entries, %presentation);
    for my $item (@{$feed->{items}}) {
        die 'invalid source entry' unless ref($item) eq 'HASH';
        my $type = $item->{type} // '';
        my $play = !ref($item->{play}) ? ($item->{play} // '') : '';
        my $media = !ref($item->{url}) && ($item->{url} // '') =~ /^[a-zA-Z][a-zA-Z0-9+.-]*:/
            && ($type eq 'audio' || ((!$type || $type eq 'link') && length($play) && $play eq $item->{url}));
        $item = {%{Chilpa::Metadata->cached($item->{url})}, %$item} if $media;
        my $openable = ref($item->{url}) eq 'CODE' || ref($item->{items}) eq 'ARRAY';
        my $shown = Chilpa::Presentation::describe($item, $media, $openable);
        if (!$media && length($around) && $shown->{stable_id} eq $around) {
            @$shown{qw(entity stable_id)} = ($openable ? 'folder' : 'notice', '');
        }
        my $id = $media ? $item->{url} : $self->remember($item, $shown->{stable_id} ? "$shown->{entity}|$shown->{stable_id}" : undef);
        my $duration = $item->{duration} // 0;
        $duration = 0 unless !ref($duration) && $duration =~ /^\d+(?:\.\d+)?$/;
        $presentation{$id} = $shown;
        my ($image) = grep { !ref($_) && ($_ // '') =~ m{^https?://} } @$item{qw(image icon cover)};
        push @entries, {
            id => $id, title => Chilpa::Presentation::text($item->{line1} // $item->{name} // $item->{title}),
            subtitle => Chilpa::Presentation::text($item->{line2} // $item->{name2}), description => Chilpa::Presentation::text($item->{description}),
            kind => $media ? 'item' : $type eq 'search' ? 'search' : $openable ? 'folder' : 'item',
            playable => $media ? JSON::PP::true : JSON::PP::false,
            why_not => $media || $openable ? '' : 'This entry does not expose a supported action.',
            duration_ms => int($duration * 1000),
            image_url => $image,
        };
    }
    my $page_size = scalar @entries;
    if (!length($input->{at} // '') && keys %{$Chilpa::Contributions::ACTIVE->entries('search')}
        && !grep { $_->{kind} eq 'search' } @entries) {
        push @entries, { id => 'bridge:search', title => 'Search', kind => 'search', playable => JSON::PP::false };
    }
    my $offset = 0 + ($feed->{offset} // $input->{cursor} // 0);
    my $total = $feed->{total} // $page_size;
    my $next = $page_size && $total > $offset + $page_size ? '' . ($offset + $page_size) : undef;
    return { title => Chilpa::Presentation::text($feed->{title} // $title), entries => \@entries, next => $next, columns => [], presentation => \%presentation };
}

1;
