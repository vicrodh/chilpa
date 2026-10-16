package Slim::Utils::PluginManager;

use strict;
use warnings;

our ($NAMESPACE, $INSTALL);

sub dataForPlugin {
    my ($class, $plugin) = @_;
    return unless defined($NAMESPACE) && index($plugin, "Plugins::${NAMESPACE}::") == 0;
    return $INSTALL;
}

sub isEnabled {
    my ($class, $plugin) = @_;
    return defined($NAMESPACE) && $plugin eq "Plugins::${NAMESPACE}::Plugin" ? 1 : 0;
}

1;
