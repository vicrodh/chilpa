package Chilpa::Client;
use strict;
use warnings;

our $CURRENT;
sub current { $CURRENT ||= bless {data => {}}, __PACKAGE__ }
sub id { 'chilpa' }
sub master { $_[0] }
sub syncGroupActiveMembers { ($_[0]) }
# No device limit is imposed by the compatibility worker. The host owns
# decoding and negotiates its actual output capabilities independently.
sub maxSupportedSamplerate { undef }
sub playingSong { undef }
sub currentPlaylistUpdateTime { $_[0]{updated} = $_[1] if @_ > 1; $_[0]{updated} }
sub pluginData {
    my ($self, $key, $value) = @_;
    $self->{data}{$key} = $value if @_ > 2;
    return $self->{data}{$key};
}
1;
