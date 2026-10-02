package Chilpa::Presentation;

use strict;
use warnings;
use JSON::PP ();

sub text { defined($_[0]) && !ref($_[0]) ? substr("$_[0]", 0, 8192) : '' }
sub number { defined($_[0]) && !ref($_[0]) && $_[0] =~ /^\d{1,9}$/ ? 0 + $_[0] : 0 }

sub quality {
    my ($item) = @_;
    # These are the host ABI's technical fields, not catalog-specific keys.
    # Never parse decorated titles or infer an album from one of its tracks.
    my %quality;
    for my $pair ([samplerate => 'sample_rate_hz'], [samplesize => 'bit_depth'],
                  [channels => 'channels']) {
        my $value = number($item->{$pair->[0]});
        next if $pair->[0] eq 'samplerate' && $value < 1000;
        $quality{$pair->[1]} = $value if $value;
    }
    my $format = text($item->{content_type});
    $quality{format} = $format if length($format) && length($format) <= 128;
    # Menu bitrates may already be localized strings. Preserve their units;
    # an unqualified number does not establish bits/s versus kbits/s.
    my $bitrate = text($item->{bitrate});
    $quality{bitrate_label} = $bitrate if length($bitrate) && length($bitrate) <= 128;
    return \%quality;
}

sub shared_quality {
    my ($quality) = @_;
    return { codec => $quality->{format}, map { $_ => $quality->{$_} }
        qw(sample_rate_hz bit_depth channels) };
}

sub describe {
    my ($item, $media, $openable) = @_;
    my $info = ref($item->{itemActions}) eq 'HASH' && ref($item->{itemActions}{info}) eq 'HASH'
        ? $item->{itemActions}{info}{fixedParams} : {};
    $info = {} unless ref($info) eq 'HASH';
    # The declared entity in an information action is more precise than the
    # menu's interaction type: an album often uses a playlist callback.
    my $entity = text($info->{type} || $item->{type});
    my %context;
    for my $part (@{ref($item->{passthrough}) eq 'ARRAY' ? $item->{passthrough} : []}) {
        next unless ref($part) eq 'HASH';
        for my $key (keys %$part) {
            $context{lc($1)} = text($part->{$key}) if $key =~ /^(album|artist|playlist)_?id$/i && !ref($part->{$key});
        }
    }
    # Context keys describe entities without inspecting callback names,
    # translated labels, protocol schemes or provider identities. Ambiguous
    # context remains a generic menu instead of guessing from its title.
    $entity = (keys %context)[0] if !$info->{type} && keys(%context) == 1
        && (!$entity || $entity eq 'playlist' || $entity eq 'link');
    $entity = 'track' if $media;
    $entity = $openable ? 'folder' : 'notice' unless $entity =~ /^(?:track|album|artist|playlist|search)$/;
    my @actions;
    if (ref($item->{itemActions}) eq 'HASH') {
        for my $key (sort keys %{$item->{itemActions}}) {
            next if @actions >= 32;
            push @actions, { key => text($key), enabled => JSON::PP::false,
                reason => 'This action requires an unsupported command interface.' };
        }
    }
    my %metadata;
    for my $key (qw(artist album genre composer work displayWork isrc musicbrainz_recording_id)) {
        my $value = $item->{$key};
        $value = $value->{name} // $value->{title} if ref($value) eq 'HASH';
        $metadata{$key} = text($value) if defined($value) && !ref($value);
    }
    return {
        entity => $entity, title => text($item->{line1} || $item->{name} || $item->{title}),
        secondary => text($item->{line2} || $item->{name2}),
        stable_id => text($item->{favorites_url} || ($media ? $item->{url} : $context{$entity} ? "$entity:$context{$entity}" : '')),
        sort_key => text($item->{textkey}), year => number($item->{year}),
        track_number => number($item->{tracknum}), disc_number => number($item->{media_number}),
        disc_count => number($item->{media_count}), metadata => \%metadata, actions => \@actions,
        advertised_quality => quality($item),
        can_preview => $openable && !$item->{nextWindow} && !$item->{input} && ($item->{type} // '') !~ /^(?:search|password|textarea|text)$/ ? JSON::PP::true : JSON::PP::false,
        can_open => $openable ? JSON::PP::true : JSON::PP::false,
        play_all => $item->{playall} ? JSON::PP::true : JSON::PP::false,
    };
}

1;
