package Slim::Plugin::Base;

use strict;
use warnings;
use Slim::Utils::PluginManager;

sub _pluginDataFor {
    my ($class, $key) = @_;
    my $data = Slim::Utils::PluginManager->dataForPlugin($class) || {};
    return $data->{$key};
}

1;
